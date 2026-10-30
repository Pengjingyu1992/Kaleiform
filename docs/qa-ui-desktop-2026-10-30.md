# Windows/Linux 界面与字体审查

本轮以仓库 Mac 版的原生字体层级、52 点顶栏、紧凑文档标签、300 点检查器、中性深浅主题和蓝色强调为参照。
Mac 参考来自 `theme.rs` 的 SF/PingFang 与 AppKit 路径，以及 `docs/macos-workspace.md`、
`docs/native-appearance-color.md` 的既有验收记录。本轮没有 Mac 设备，不能视作三端同机截图对照。

## 流程与发现

1. **进入工作区、阅读文档标题与属性：需要修正 → 已修正。**
   本轮先在真实 Linux 桌面捕获 `before/01-home.png`、`02-properties-dark.png`、
   `03-properties-light.png`、`04-inspector-narrow.png`。中文控件与 Source Sans 3 拉丁字符的字面大小、
   文档标签的粗细不够协调；Windows/Linux 界面改用仓库已有的 OFL Inter Regular/Medium/Semibold，
   文档标签用 Medium，区块标题保留 Semibold。中文回退的基线随实际拉丁 UI 字体计算；
   可变字体回退保留命名实例轴，避免选择的字重失效。简体中文优先使用本机 Microsoft YaHei UI（Windows）或 Noto Sans CJK SC（Linux），避免误取日文字形；缺少时保留通用回退。符号继续由现有开源字体回退覆盖。
   根面板也参与字形扫描；同一回退字体后来加入另一 UI 字重时使用独立登记名，避免 egui 忽略已有名称而漏掉字体族链接。这修复了正文正常、文档标签仍显示方框的问题。
2. **打开新建文档、选择分类：需要修正 → 已修正。**
   `before/05-new-document.png` 在 800×600 窗口显示残缺的下一项分类文字。
   分类宽度不足时显示完整的当前分类及分类菜单；宽度足够时继续显示标签栏。
   英文、简体中文、法文的紧凑布局回归检查分类文字不被裁切，菜单选择与取消仍有指针回归。
3. **编辑属性、查看描摹状态与缩放：回归发现问题后修正。**
   Inter 的度量暴露了变换组旁九点定位器的固定高度偏移；改为按实际输入行高居中；数值与文本输入统一内边距，保持外部列宽一致，同时为分数值和单位留出空间。
   描摹状态行的取消操作在短面板的可见性继续由原有严格边界与点击测试约束。
   新增跨平台原生截图流程，保存深/浅主题、800 点窄检查器、新建文档、150%/200% UI 缩放及字体栈记录；
   等待中文回退实际安装后截图，避免把加载中的缺字框当成完成态。

控件仍使用统一圆角、留白、分隔线和中性表面；没有增加装饰性阴影或替换工具图标。
字体变化只作用于界面，不改画布作品、文件内字体选择、几何精度或 Mac 系统字体优先级。

## 验证证据

- 捕获脚本：`tests/desktop_visual.py`，由 Windows 安装/便携与 Linux DEB 的扩展桌面验收共同执行。
- 截图命名：`ui-audit/properties-dark.png`、`properties-light.png`、`inspector-narrow.png`、
  `new-document-compact.png`、`new-document-1.5.png`、`new-document-2.0.png`；旁附 `ui.inspect` JSON。
- 本地 Linux 真实桌面扩展矩阵：414 项通过、0 失败；UI 专项回归：948 项通过、0 失败、1 项默认忽略。
- 本轮已实际查看上述六张修正后 Linux 截图：中文标签不再缺字，分类没有残片；深浅主题、窄检查器及缩放后的关闭/创建按钮正常。
- `RUST_TEST_THREADS=1 cargo xtask ci`：七项门禁全部通过，4,803 项测试通过、0 失败、5 项保留默认忽略，20 个 WASM 模块检查通过。Windows 原生构建结果在发布前另核对。
- Windows 完整工作区测试改用常规 test profile，避免对数千项测试逐一进行 release ThinLTO，
  原有断言和覆盖范围保留；MSI、便携包运行及性能测量继续使用 release 二进制。

## 隐私与许可

没有新增字体文件。Inter、Source Sans 3、JetBrains Mono 均沿用 `ASSETS.md`/`NOTICE` 中的现有开源许可；
不复制、下载或分发 Apple 的 SF、PingFang、系统图标或用户字体。Mac 仅在运行时读取其本机系统字体。
验收仅生成原创合成图形和文字，不使用用户作品、真实最近文件、账号或偏好；运行时禁用偏好持久化。
截图是测试证据，未嵌入应用，也不参与作品上传；没有新增遥测、网络服务或凭据绑定。
最终包继续执行分发隐私门禁、许可证、路径扫描和 SHA-256 校验。Mac 已有安装包、Release 附件和 latest 标记保留。

## 验收边界

Linux 截图使用 X11/Vulkan 软件渲染。Windows 实际运行以隔离原生 CI 结果为准，
物理显示器、ClearType、多显示器 DPI、输入法、数位板及 GPU 驱动仍需设备验收。
苹果式的排版与层级是本次设计目标，不代表 Apple 官方设计或认证。
