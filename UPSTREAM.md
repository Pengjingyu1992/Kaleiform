# 上游来源与版本边界

- 上游项目：[storytold/vectorcraft](https://github.com/storytold/vectorcraft)。
- 当前版本基线：[`21355945ea2a4d1136da4deed36d07b6aca21c52`](https://github.com/storytold/vectorcraft/commit/21355945ea2a4d1136da4deed36d07b6aca21c52)，2026-10-10 同步，虹构预览版 `0.7.0-kaleiform.4`。
  这是官方 `main` 的固定提交，包含 `v0.7.0` 标签之后的改动，不等于仅同步官方发布包。
- 首版上游基线：[`8b2be144cc964f9ac26eb175484b887cef0e86db`](https://github.com/storytold/vectorcraft/commit/8b2be144cc964f9ac26eb175484b887cef0e86db)，工作区版本 `0.4.0`。
- 同步前虹构快照：`42f04be96c59494639e9761c472acb1a1680845f`，预览版 `0.4.0-kaleiform.3`。
- 2026-10-08 发布准备时检查到的上游 main：[`d9fc3b6505bb185c7ecce05ab4b60b86da12822f`](https://github.com/storytold/vectorcraft/commit/d9fc3b6505bb185c7ecce05ab4b60b86da12822f)，工作区版本 `0.5.0`。
  此基线之后的提交不包含在虹构首个预览版中；链接仅用于可追溯，不承诺其后一直是最新版本。

新仓库以经过隐私审查的源码快照开始，不包含原开发目录的 `.git` 或旧提交历史。
原本地仓库及历史保持不变。首个提交不是全部代码的原创声明，代码来源、原许可证和公开贡献者署名均保留。

## 保留的兼容项

`vectorcraft` / `vectorcraft-cli` Cargo 包与 CLI 名称、部分环境变量、
`ai.storyteller.vectorcraft` 包标识、`.vectorcraft` / `.drawcraft` 文档后缀、
旧设置及恢复目录保留，是为了不损坏已有文档和升级路径。
这些名称不是上游官方发行的声明。

同机安装上游和虹构时可能发生设置共享、文件关联或应用识别冲突；首版不声称两者完全隔离。
后续独立应用标识与数据迁移需要单独设计和测试，不能仅靠替换字符串完成。

具体功能差异见 [docs/differences.md](docs/differences.md)，作者致谢见 [ACKNOWLEDGEMENTS.md](ACKNOWLEDGEMENTS.md)。

## 2026-10-10 集成策略

- 核心功能采用上述官方固定提交；密集描摹轮廓保护、并行描摹、复杂度限制和渲染恢复等采用官方实现。
- 保留虹构品牌、中文字体字重回退、补充标点禁则、短屏布局、批量选择查询、忽略白色孔洞和异常 PDF 坐标保护。
- 沿用 APFS DMG、中文 App 名称、无白边图标和隐私路径映射；不引入官方 DMG 品牌背景。
- 不使用无共同历史的普通合并猜测基线；从明确首版基线执行三方补丁集成。
- 验证及范围记录见 [集成说明](docs/upstream-integration-2026-10-10.md)。
