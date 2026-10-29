# Windows x64 预览版

Windows 与更新的 Linux 使用仓库 macOS 预览 17 的同源工作区：紧凑文档标签、画布优先布局、
属性/图层/库/历史记录侧栏、窄窗口浮出检查器、统一的新建文档窗口和中性浅深色主题。
Windows/Linux 保留右侧最小化、最大化/还原和关闭按钮；完整菜单独立成行，关闭未保存文件时先询问。
macOS 继续使用系统菜单和原生窗口材质。Windows/Linux 的窗口底色保持不透明，浮动面板复用画布模糊，
不声称支持 AppKit 的桌面透视效果。

## 获取试用构建

在[桌面预览 17 Release](https://github.com/Pengjingyu1992/Kaleiform/releases/tag/desktop-v0.7.0-kaleiform.17)
下载 MSI、便携 ZIP 及 Windows `.sha256` 清单。它使用独立标签，保留已有 Mac 附件与 latest 标记。

在 [Windows x64 packages](https://github.com/Pengjingyu1992/Kaleiform/actions/workflows/windows.yml)
的构建页，先登录 GitHub，再下载 Artifacts 中的 `kaleiform-windows-x64`。它在原生测试、
PE/版本、SHA-256 和隐私检查通过后保存；安装版与便携版的运行验收是另外两个任务，
请同时查看它们的结果，完整通过的任务才代表本轮自动验收成功。包中包含：

- `vectorcraft-0.7.0-kaleiform.17-windows-x64.msi`：Windows 10/11 x64 安装包。
- `vectorcraft-0.7.0-kaleiform.17-windows-x64-portable.zip`：解压后运行 `vectorcraft.exe`；
  `vectorcraft-cli.exe` 用于命令行和 MCP。
- `vectorcraft-0.7.0-kaleiform.17-windows-x64.sha256`：以上两个文件的 SHA-256 清单。

使用 PowerShell `Get-FileHash 文件名 -Algorithm SHA256` 核对清单后安装或解压。
当前试用构建没有配置 Authenticode 证书，属于未签名预览包。常规构建保存 Actions 产物；
本次经授权的独立发布固定源码、Windows/Linux/源码 CI 与打包检查记录，全部成功后才上传，
不替换任何已有发布资产。

MSI 安装到 Program Files 下的 Kaleiform 文件夹，提供开始菜单入口、CLI，以及
`.vectorcraft`、`.drawcraft` 和 `.vctemplate` 的打开关联。不会注册 SVG/PDF 的默认应用。
便携包和 MSI 都保留项目、上游、字体与第三方许可证；不附带测试作品、截图、偏好或日志。
UI 中文通过 Windows 系统字体回退；没有复制或内嵌 Windows/macOS 的系统字体。

## 从源码构建

需要 Rust 1.95+ 的 MSVC 工具链、Visual Studio C++ Build Tools、Windows SDK 和 PowerShell。
生成 MSI 还需要 .NET 与 WiX 5：

```powershell
rustup target add x86_64-pc-windows-msvc
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.UI.wixext/5.0.2 WixToolset.Util.wixext/5.0.2
./packaging/windows/package.ps1 -Arch x64
```

编译静态链接 VC 运行库，检查两个 PE 的架构与子系统、应用版本及资源信息，并重映射源码和
工具路径。MSI 和 ZIP 的生成结果只包含本轮构建文件。

## 验证

Windows CI 在 Windows 2022 上执行共享 UI 与桌面代码测试，生成 MSI/ZIP 并通过静态隐私检查，
然后保存包。安装版与便携版分别使用新的 Windows 2025 runner，保持默认 DX12 后端，
用 `packaging/windows/verify.ps1 -Phase ...` 分步校验 SHA-256、安装 MSI、检查文件关联与许可证，执行 CLI
绘图、原生重开和 SVG/PNG/PDF 导出，以及真实窗口的中文、浅深色、窄窗口、新建取消和未保存关闭流程，
并卸载本轮安装。卸载需要本轮安装写入的标记，拒绝卸载其他已有安装。
诊断日志分别保存为 `kaleiform-windows-x64-build-qa`、`kaleiform-windows-x64-installed-qa`、
`kaleiform-windows-x64-portable-qa`；运行日志与合成截图不进入安装包。
CLI 与 MSI 客户端有独立超时，每个运行步骤最多 5 分钟、每台运行验收机器最多 20 分钟。
阶段记录会显示在任务的 Annotations 中。单机全部验收入口 `-Phase All` 仍可用，但仅限一次性测试机器。

这些自动检查不代表所有 Windows 显卡、输入法、数位板和多显示器 DPI 组合都已实测。
本轮原生结果见[通过的 Windows 验收](https://github.com/Pengjingyu1992/Kaleiform/actions/runs/40903260548)。
ARM64 和实验性的 [Windows 7 入口](windows7.md) 不在本次 x64 交付验收范围内。
