# 虹构 macOS 预览 19

版本 `0.7.0-kaleiform.19`，面向 Apple Silicon。

- EPS 与 PDF 导入保留三角形等非矩形剪裁，即便它们的外接矩形恰好覆盖整页。
- DXF 2000–2018 导出写入正确的图纸布局字段与模型空间标志。
- “变换”面板按实际输入框高度居中对齐参考点网格和比例锁定按钮。
- 密集路径拖动共享绘制边界。合并前在 Apple M3 Pro 上测量同一个 5 万路径样本，预览准备的耗时中位数由 33.30 ms 降至 17.40 ms。计时范围是准备拖动预览。
- 原生文档在锚点或对象较多时使用紧凑 JSON。同一份 2 万锚点样本从 3.98 MB 减至 0.65 MB，解码后的文档数据完全一致。

安装包采用 ad-hoc 签名，尚未经过 Apple 公证。DMG 内含应用与 CLI。
使用 Release 附件中的 `SHA256SUMS.txt` 核对安装包，使用 `BUILD-INFO.json` 查看源码提交和构建信息。

[下载安装包](https://github.com/Pengjingyu1992/Kaleiform/releases/tag/v0.7.0-kaleiform.19)
 · [安装指南](installation.md)
 · [隐私说明](privacy.md)
