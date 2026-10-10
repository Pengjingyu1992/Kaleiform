# 安装虹构 · Kaleiform

## 预览版支持范围

`0.7.0-kaleiform.4` 是早期预览版，提供 macOS **arm64 / Apple Silicon** 包，
最低系统声明为 macOS 11，实际交互测试在维护者当前 macOS 上进行，未逐一测试所有系统版本。
Intel Mac 不能直接使用此 arm64 包。Windows、Linux、FreeBSD 和 Web 暂无本项目已验证的下载包。

## macOS 安装

1. 从 [本仓库 Releases](https://github.com/Pengjingyu1992/Kaleiform/releases) 下载
   `kaleiform-0.7.0-kaleiform.4-macos-arm64.dmg` 及 `SHA256SUMS.txt`。
2. 可在下载目录运行下面命令，将结果与 `SHA256SUMS.txt` 中同名文件比较：

   ```sh
   shasum -a 256 kaleiform-0.7.0-kaleiform.4-macos-arm64.dmg
   ```

3. 双击 DMG，把 **虹构.app** 拖至窗口中的 **Applications** 文件夹。
   避免直接在 DMG 内运行。升级已有应用前先退出虹构并备份作品。
4. 从“应用程序”打开 **虹构**。

### macOS 安全提示

本版仅使用 ad-hoc 签名以保护包内部的一致性，**不具备 Apple 验证开发者身份的 Developer ID 签名，
也未经过 Apple 公证**。因此，下载后可能出现“无法验证开发者”或类似安全提示。

确认文件来自本仓库并核对校验值之后，先尝试打开一次，再前往
“系统设置 → 隐私与安全性”，按系统提供的“仍要打开”继续。
不同 macOS 版本的按钮文字可能略有区别，相关授权请在本机完成。
如果系统明确提示恶意软件或文件已损坏，请停止运行并反馈，不要忽略这类警告。
我们不要求关闭 Gatekeeper、不要求全局禁用安全检查，也不要求在聊天中提供系统密码。

SHA-256 只能检测文件与校验清单是否一致，不能代替开发者身份认证；请同时核对仓库地址。

### ZIP 备选

不能挂载 DMG 时，可下载 `kaleiform-0.7.0-kaleiform.4-macos-arm64.zip`，
双击解压，把 **虹构.app** 移入“应用程序”。不要只移动 `.app` 内部的可执行文件。
ZIP 与 DMG 包含同一份 App，签名与系统安全提示相同。

### 中文界面和字体

中文系统自动选择中文，其他系统可用“语言 / Language”或“首选项 → 用户界面 → 语言 → 简体中文”。
本版不内嵌额外 craft-fonts 字体；使用本机已安装字体进行 CJK 回退，不会将苹方等系统字体复制到发布包。
缺字时请安装覆盖所需字符且许可证允许使用的字体，再重启应用。

### 与旧版本共存

为保持旧文件和设置可用，本版沿用部分 VectorCraft 包标识、设置路径以及
`.vectorcraft` / `.drawcraft` 后缀。**同机安装两者不意味着设置隔离**，可能共用设置或影响文件关联。
请先备份，不要把名字改变当作格式迁移。卸载 App 本身不会自动删除原有作品或设置。

## 从源码编译

### macOS

安装 Git、Rust 1.95+ 和 Xcode Command Line Tools（已安装则无需重复）：

```sh
xcode-select --install
git clone https://github.com/Pengjingyu1992/Kaleiform.git
cd Kaleiform
cargo run --release --locked -p vectorcraft
```

构建 App：

```sh
cargo xtask bundle
open dist/虹构.app
```

`cargo xtask bundle` 是开发构建，不会自动取得 Apple Developer ID 或公证。
Cargo 包名仍是 `vectorcraft`，App 显示名称为虹构。

### Windows / Linux（源码入口，未实机验证）

Windows 需要 Rust MSVC 工具链与 Visual Studio C++ Build Tools。
Linux 桌面还需要对应的窗口、图形及对话框开发库，例如 Ubuntu 可安装：

```sh
sudo apt-get install build-essential pkg-config libxkbcommon-dev libwayland-dev \
  libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev
git clone https://github.com/Pengjingyu1992/Kaleiform.git
cd Kaleiform
cargo run --release --locked -p vectorcraft
```

这些是继承的构建入口，发行版依赖可能不同；不是已经在所有系统验证的安装承诺。

### 可选内嵌开源 CJK 字体

源码可读取独立 [craft-fonts](https://github.com/storytold/craft-fonts) 仓库。
字体受各自的 SIL OFL 等许可约束；发布时必须附上所嵌入字体的完整许可和署名。

```sh
git clone https://github.com/storytold/craft-fonts ../craft-fonts
CRAFT_FONTS_DIR="$PWD/../craft-fonts" cargo xtask bundle
```

未设置该选项也能编译和运行；本版二进制发布包采用未设置该选项的构建。

### CLI 与 MCP

```sh
cargo build --release --locked -p vectorcraft-cli
cargo run --release --locked -p vectorcraft-cli -- --help
cargo run --release --locked -p vectorcraft-cli -- mcp --headless
```

macOS App 内也包含 `Contents/MacOS/vectorcraft-cli`。
MCP 不要求本项目的账号或 API key，但你选择的外部 AI 客户端可能要求自己的账号或密钥。
不要将密钥写进源码、Issue、截图或公开配置。
接口说明见 [MCP](mcp.md) 和 [控制协议](control-protocol.md)。

### Web 与检查

```sh
rustup target add wasm32-unknown-unknown
cargo xtask ci
cargo install trunk --locked
cd apps/vectorcraft-web
trunk build --release
```

Web 入口继承自上游，目前仅作源码提供，尚无本项目部署的在线服务。
