# Linux 预览版

Linux 版使用与 macOS 相同的 Rust 绘图引擎和中文界面，保留 `vectorcraft` 二进制、
`ai.storyteller.vectorcraft` 桌面标识及旧文档扩展名以兼容既有文件和设置。
这是早期预览版；已公开的下载文件以 Releases 的实际资产为准。
本次构建、最小环境安装及 X11/Wayland 功能结果见 [Linux 验收记录](qa-linux-2026-10-13.md)。

## 安装

优先使用 x86_64 的 Ubuntu 22.04 以上版本。安装包由 Ubuntu 22.04 / glibc 2.35 构建；
打包前检查两个 ELF 的架构、版本及 glibc 符号要求，使用更新系统构建时会相应提高包依赖。
Debian、其他桌面环境和显卡组合需要各自验证，不能只凭 glibc 版本判断全部功能可用。

取得安装包及同版 `.sha256` 清单后，先在该目录验证：

```sh
sha256sum --check vectorcraft-0.7.0-kaleiform.6-linux-x86_64.sha256
sudo apt install ./vectorcraft-0.7.0-kaleiform.6-linux-x86_64.deb
```

安装后在应用菜单启动 **虹构 / Kaleiform**，或运行 `vectorcraft`。
`.vectorcraft`、旧 `.drawcraft` 和 `.vctemplate` 文件会注册到桌面文件关联；
包不会替用户强制修改 SVG / PDF 的默认应用。
卸载使用 `sudo apt remove vectorcraft`，保留用户自己的作品和本地设置。

便携目录无需 root 权限：

```sh
tar -xzf vectorcraft-0.7.0-kaleiform.6-linux-x86_64.tar.gz
./vectorcraft-0.7.0-kaleiform.6-linux-x86_64/bin/vectorcraft
```

AppImage 可执行后运行；没有 FUSE 的系统可以提取运行：

```sh
chmod +x vectorcraft-0.7.0-kaleiform.6-linux-x86_64.AppImage
APPIMAGE_EXTRACT_AND_RUN=1 ./vectorcraft-0.7.0-kaleiform.6-linux-x86_64.AppImage
```

便携目录和 AppImage 同样需要系统的图形运行库：xkbcommon（含 X11 模块）、X11 或 Wayland，
以及 Vulkan / EGL 驱动。它们携带应用、图标和许可证，不捆绑系统驱动或用户设置。
RPM 构建入口保留；Fedora/RHEL、ARM64 和 Flatpak 的运行结果需单独验证。

## 中文、显示和文件选择

中文系统自动选择简体中文，也可通过 Language 菜单或首选项切换。
未内嵌可选 craft-fonts 时，中文字体来自用户系统；Ubuntu 可安装 `fonts-noto-cjk`。
系统字体不会复制到发行包。Open/Save 对话框使用桌面的 `xdg-desktop-portal` 服务，
GNOME/Xfce 通常使用 GTK portal，KDE 使用 KDE portal。

X11 和 Wayland 的窗口入口均保留。继承的 winit 0.30 在 Wayland 下尚不支持文件拖放和
原生数位板输入；需要这些操作时可用 XWayland：`WAYLAND_DISPLAY= vectorcraft`。
详见 [Linux 开发说明](development.md#linux-wayland-and-x11)。
磨砂玻璃的原生窗口材质仍仅在 macOS 实现。

## 构建与验收

需要 Rust 1.95 以上版本、C 编译工具、Python 3 和 binutils。Ubuntu 的编译依赖：

```sh
sudo apt install build-essential pkg-config python3 binutils libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev \
  libx11-dev libx11-xcb1 libxcursor1 libxrandr-dev libxi-dev libgl1-mesa-dev
packaging/linux/package.sh --formats tar
```

生成 deb/rpm 还需要 nFPM；AppImage 需要 appimagetool。默认命令生成全部格式：
`packaging/linux/package.sh`。`--skip-build` 只复用通过架构和版本检查的 release 二进制。
每次生成的 `.sha256` 清单仅包含本次选择的文件，不会把旧包混入结果。
要保持 glibc 2.35 基线，请在 Ubuntu 22.04 构建，不要直接把更新发行版的二进制标为同样兼容。
所有格式均保留 NOTICE、资产署名、上游来源及第三方许可证。

安装包后的功能验收：

```sh
python3 packaging/linux/smoke.py
LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=vulkan xvfb-run -a \
  python3 packaging/linux/smoke.py --desktop --output /tmp/kaleiform-smoke
```

脚本检查绘图、SVG/PDF/PNG 导出、原生文档重开；桌面模式还检查以文件参数启动、中文界面、
撤销重做、画板渲染和真实窗口截图。它只启动和停止自己的应用进程。
可用 `--prefix` 验收解压后的便携目录，用 `--output` 保留截图和合成作品。

`.github/workflows/linux.yml` 在 Ubuntu 22.04 构建包，在独立最小 Ubuntu 容器中安装并验收 deb、
卸载，再验收便携包。
发布流程也会对安装后的桌面运行功能检查。执行 `cargo xtask ci` 仍是源码质量门槛；
构建成功、窗口启动和实际功能验证应分别记录。
