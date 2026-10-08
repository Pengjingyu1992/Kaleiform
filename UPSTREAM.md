# 上游来源与版本边界

- 上游项目：[storytold/vectorcraft](https://github.com/storytold/vectorcraft)。
- 已同步基线：[`8b2be144cc964f9ac26eb175484b887cef0e86db`](https://github.com/storytold/vectorcraft/commit/8b2be144cc964f9ac26eb175484b887cef0e86db)，工作区版本 `0.4.0`。
- 本地修复快照参考提交：`df72baab3f263fa90dbf44ee9de469c00d2cea8d`，另包含品牌、图标及中文应用名称的未提交本地改动。
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
