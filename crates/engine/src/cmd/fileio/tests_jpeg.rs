//! JPEG Options: colour model, coding method, scans, profile and image maps.

use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 60, "height": 40})).unwrap();
    s
}

fn b64(v: &Value) -> Vec<u8> {
    vectorcraft_format::base64_decode(v["dataBase64"].as_str().expect("dataBase64")).unwrap()
}

/// The JPEG's SOF marker and the number of components in its frame header.
fn frame(file: &[u8]) -> (u8, u8) {
    let mut i = 2;
    while !(0xC0..=0xC2).contains(&file[i + 1]) {
        i += 2 + u16::from_be_bytes([file[i + 2], file[i + 3]]) as usize;
    }
    (file[i + 1], file[i + 9])
}

#[test]
fn jpeg_options_reach_the_file() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 30, "height": 20})).unwrap();
    let jpg = |s: &mut Session, p: Value| b64(&s.execute("document.export", &merge(json!({"format": "jpg"}), p)).unwrap());
    assert_eq!(frame(&jpg(&mut s, json!({}))), (0xC0, 3), "baseline RGB by default");
    assert_eq!(frame(&jpg(&mut s, json!({"colorModel": "cmyk"}))), (0xC0, 4));
    assert_eq!(frame(&jpg(&mut s, json!({"colorModel": "grayscale"}))), (0xC0, 1));
    assert_eq!(frame(&jpg(&mut s, json!({"method": "progressive", "scans": 5}))).0, 0xC2);
    let icc = |f: &[u8]| f.windows(12).any(|w| w == b"ICC_PROFILE\0");
    assert!(icc(&jpg(&mut s, json!({}))) && !icc(&jpg(&mut s, json!({"embedIcc": false}))));
    assert!(jpg(&mut s, json!({"quality": 0})).len() < jpg(&mut s, json!({"quality": 100})).len());
    for bad in [json!({"colorModel": "lab"}), json!({"method": "fast"}), json!({"imageMap": "both"})] {
        assert!(s.execute("document.export", &merge(json!({"format": "jpg"}), bad.clone())).is_err(), "{bad}");
    }
    let r = s.execute("document.formats", &json!({})).unwrap();
    let jpg = r["formats"].as_array().unwrap().iter().find(|f| f["id"] == "jpg").unwrap();
    for k in ["colorModel", "method", "scans", "embedIcc", "imageMap"] {
        assert!(jpg["options"].get(k).is_some(), "{k}");
    }
}

#[test]
fn progressive_cmyk_export_reopens_renders_samples_and_traces() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 5, "y": 5, "width": 50, "height": 30})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#3366ff"})).unwrap();
    for quality in [40, 80, 100] {
        for scans in 3..=5 {
            let params = json!({"format": "jpg", "colorModel": "cmyk", "method": "progressive", "quality": quality, "scans": scans});
            let jpg = b64(&s.execute("document.export", &params).unwrap());
            s.execute("document.open", &json!({"name": "qa.jpg", "dataBase64": vectorcraft_format::base64_encode(&jpg)})).unwrap();
            let doc = &s.doc().unwrap().doc;
            let blob = doc.images.values().next().unwrap();
            let sample = blob.sample(20.0, 20.0, 1).expect("eyedropper must read the JPEG");
            assert!(sample[2] > sample[0] + 80, "{params}: {sample:?}");
            assert!(blob.map_rgb(|_| [255, 0, 0]).is_some(), "recolour must decode the JPEG");
            let shown = vectorcraft_render::Renderer::new().render_region(doc, doc.artboards[0].rect, 1.0, true).pixel(20, 20);
            assert!(shown[2] > shown[0] + 80, "reopened JPEG is visible: {params}: {shown:?}");
            s.execute("select.all", &json!({})).unwrap();
            let traced = s.execute("imageTrace.make", &json!({})).unwrap();
            assert!(traced["paths"].as_u64().unwrap() > 0, "{params}");
            s.execute("file.close", &json!({})).unwrap();
        }
    }
}

#[test]
fn a_jpeg_without_pixel_data_cannot_open_or_place_blank_art() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 5, "y": 5, "width": 20, "height": 10})).unwrap();
    let mut jpg = b64(&s.execute("document.export", &json!({"format": "jpg"})).unwrap());
    let scan = jpg.windows(2).position(|bytes| bytes == [0xFF, 0xDA]).unwrap();
    let header_len = usize::from(u16::from_be_bytes([jpg[scan + 2], jpg[scan + 3]]));
    jpg.truncate(scan + 2 + header_len);
    // The valid header still reports dimensions; the entropy-coded pixels are absent.
    assert!(image::ImageReader::new(std::io::Cursor::new(&jpg)).with_guessed_format().unwrap().into_dimensions().is_ok());
    let before = vectorcraft_testkit::invariants::doc_json(&s.doc().unwrap().doc);
    for command in ["document.open", "file.place"] {
        let docs = s.documents().len();
        assert!(s.execute(command, &json!({"name": "empty-scan.jpg", "dataBase64": vectorcraft_format::base64_encode(&jpg)})).is_err());
        assert_eq!(s.documents().len(), docs);
        assert_eq!(vectorcraft_testkit::invariants::doc_json(&s.doc().unwrap().doc), before);
    }
}

#[test]
fn optimized_rgb_jpeg_reopens_with_its_original_colours() {
    let mut s = session();
    s.execute("shape.rectangle", &json!({"x": 5, "y": 5, "width": 50, "height": 30})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#3366ff"})).unwrap();
    for method in ["baseline", "optimized", "progressive"] {
        let jpg = b64(&s.execute("document.export", &json!({"format": "jpg", "method": method, "quality": 40})).unwrap());
        s.execute("document.open", &json!({"name": "qa.jpg", "dataBase64": vectorcraft_format::base64_encode(&jpg)})).unwrap();
        let doc = &s.doc().unwrap().doc;
        let shown = vectorcraft_render::Renderer::new().render_region(doc, doc.artboards[0].rect, 1.0, true).pixel(20, 20);
        let sample = doc.images.values().next().unwrap().sample(20.0, 20.0, 1).unwrap();
        for px in [shown, sample] {
            assert!(px.iter().zip([51u8, 102, 255, 255]).all(|(a, b)| a.abs_diff(b) < 12), "{method}: {px:?}");
        }
        s.execute("file.close", &json!({})).unwrap();
    }
}

#[test]
fn optimized_cmyk_jpeg_renders_without_a_decoder_panic() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 320, "height": 240})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 20, "y": 20, "width": 130, "height": 100})).unwrap();
    s.execute("paint.setFill", &json!({"color": "#3366ff"})).unwrap();
    let jpg = b64(&s.execute("document.export", &json!({"format": "jpg", "colorModel": "cmyk", "method": "optimized", "quality": 40})).unwrap());
    s.execute("document.open", &json!({"name": "qa.jpg", "dataBase64": vectorcraft_format::base64_encode(&jpg)})).unwrap();
    let doc = &s.doc().unwrap().doc;
    let blob = doc.images.values().next().unwrap();
    assert_eq!(blob.cmyk().expect("optimized CMYK must decode").width, 320);
    let shown = vectorcraft_render::Renderer::new().render_region(doc, doc.artboards[0].rect, 1.0, true).pixel(50, 50);
    assert!(shown[2] > shown[0] + 60, "optimized CMYK remains blue: {shown:?}");
}

#[test]
fn image_maps_list_the_objects_with_a_url() {
    let mut s = session();
    // Areas cover the visual bounds: no stroke, so they are the shapes' own.
    s.execute("paint.setStroke", &json!({"none": true})).unwrap();
    s.execute("shape.rectangle", &json!({"x": 10, "y": 5, "width": 20, "height": 10})).unwrap();
    s.execute("attributes.set", &json!({"url": "https://example.com/a?b=1&c=2", "imageMap": "rectangle"})).unwrap();
    s.execute("shape.ellipse", &json!({"x": 35, "y": 20, "width": 20, "height": 10})).unwrap();
    s.execute("attributes.set", &json!({"url": "https://example.com/round", "imageMap": "polygon"})).unwrap();
    // A URL without a map shape makes no area.
    s.execute("shape.rectangle", &json!({"x": 0, "y": 30, "width": 5, "height": 5})).unwrap();
    s.execute("attributes.set", &json!({"url": "https://example.com/none"})).unwrap();

    let r = s.execute("document.export", &json!({"format": "jpg", "imageMap": "client", "ppi": 144})).unwrap();
    let linked = r["linked"].as_array().unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0]["name"], "Untitled-1.html");
    let html = String::from_utf8(b64(&linked[0])).unwrap();
    assert!(html.contains("<img src=\"Untitled-1.jpg\" width=\"120\" height=\"80\" usemap=\"#Untitled-1\""), "{html}");
    assert!(html.contains("<area shape=\"rect\" coords=\"20,10,60,30\" href=\"https://example.com/a?b=1&amp;c=2\""), "{html}");
    let poly = html.lines().find(|l| l.contains("shape=\"poly\"")).expect("a polygon area");
    assert!(poly.contains("coords=\"70,50,") && !poly.contains(",70,50\""), "the ellipse's outline, not closed twice: {poly}");
    assert!(html.find("example.com/round") < html.find("example.com/a?"), "topmost first: {html}");
    assert!(!html.contains("example.com/none"));

    let r = s.execute("document.export", &json!({"format": "jpg", "imageMap": "server"})).unwrap();
    let map = String::from_utf8(b64(&r["linked"][0])).unwrap();
    assert_eq!(r["linked"][0]["name"], "Untitled-1.map");
    assert!(map.lines().any(|l| l == "rect https://example.com/a?b=1&c=2 10,5 30,15"), "{map}");
    assert!(map.lines().any(|l| l.starts_with("poly https://example.com/round ")), "{map}");

    // Written next to the image, named after it.
    let dir = std::env::temp_dir().join(format!("vc-imagemap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pic.jpeg");
    let r = s.execute("document.export", &json!({"path": path.to_string_lossy(), "imageMap": "client"})).unwrap();
    assert_eq!(r["linked"][0], dir.join("pic.html").to_string_lossy().as_ref());
    assert!(std::fs::read_to_string(dir.join("pic.html")).unwrap().contains("src=\"pic.jpeg\""));
    let _ = std::fs::remove_dir_all(dir);
    // One file only: Export for Screens can't add the map.
    assert!(s.execute("document.exportForScreens", &json!({"formats": [{"format": "jpg", "imageMap": "client"}]})).is_err());
}
