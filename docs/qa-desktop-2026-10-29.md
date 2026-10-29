# Windows/Linux 工作区对齐验收（2026-10-29）

参考仓库 macOS 预览 17（`5fb7c53`）的工作区和语义主题实现。
使用自行生成的矩形和椭圆，不使用用户文档，也不使用外部商业绘图程序。

## 改动

- Windows/Linux 原先仍使用旧标题栏、独立标签条和面板轨。现在使用与 Mac 相同的工作区、
  紧凑文档标签、检查器、历史页签、窄窗口覆盖层和新建文档界面。
- 自绘窗口按钮拥有独立的右侧空间；完整菜单另占一行，保留原有文本输入焦点与菜单命令行为。
- 无原生语义颜色的平台使用中性色和蓝色强调，四档亮度仍有不同面板底色。窗口背板、输入框和画板
  保持不透明；已有原生颜色和减少透明度/动画设置不被默认值覆盖。
- MSI/便携包包含同样的署名与许可证，检查 PE 架构、GUI/console 子系统与版本，产生独立校验清单。
  MSI 增加原生文档关联；Windows 与 Linux 共用安装后功能验收脚本。
- 链接图片更新测试为真实 PNG 指定不同修改时间，避免毫秒内同大小改写使测试偶发无法发现修改；
  保留更新提示、确认后状态及手动更新偏好的断言，未更改应用的链接检测格式。

## Linux 本机验证

在隔离的 Xvfb 窗口、Mesa llvmpipe/Vulkan 上执行，禁用用户偏好持久化。
基线和更新后均拍摄实际窗口。已检查 1440×900 的浅深色与 800×600 的窄窗口和新建弹窗，
核对菜单、标签、窗口按钮、检查器和底部操作区。
CLI 绘图、SVG/PNG/PDF 导出、原生重开，桌面的中文切换、撤销重做、保存后重开、
SVG/PNG/PDF 后台导出、新建取消和关闭未保存提示通过。运行目录仅含合成文档。
窗口按钮、拖动、边缘缩放、DPI、原生 Mac/旧布局保留和全部面板入口由共享 UI 测试覆盖。

## 源码与 Linux 安装包

`RUST_TEST_THREADS=1 cargo xtask ci` 的 fmt、严格 Clippy、test、assets、brands、layers、wasm
七项全部通过：4,785 项测试通过、0 失败、5 项按已有标注忽略；20 个 WASM 安全模块检查通过。

提交 `bbc2847` 的 [Linux packages #16](https://github.com/Pengjingyu1992/Kaleiform/actions/runs/40864060423)
成功生成 DEB、RPM、AppImage、便携压缩包和校验清单。在最小 Ubuntu 22.04 容器中安装 DEB，
执行真实桌面与 CLI 检查、验证 MIME 类型、卸载；另检查便携包及 AppImage 的 CLI，比较二进制，
执行产物隐私扫描和 SHA-256 校验。RPM 在本轮仅验证生成，未进行 Fedora 安装验收。
产物名称为 `kaleiform-linux-x86_64`。

## Windows 原生验收

Windows 的安装与桌面结果以 [Windows x64 CI](https://github.com/Pengjingyu1992/Kaleiform/actions/workflows/windows.yml)
成功任务及其 QA 产物为准，不能把 Linux 上的共享 UI 测试记作 Windows 原生运行。

## 边界

当前不增加新的 Mac 安装包、不改变原 Mac 发布标签或资产。Windows CI 是自动化机器，
不等于真实显卡、中文输入法候选窗、数位板、多显示器或 ARM64 验收。原生 Mac 的 AppKit 材质
和系统菜单保留；Linux/Windows 使用可读的不透明背板。既有大型文档性能缺口未在本任务中优化。
