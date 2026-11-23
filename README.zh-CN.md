[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md) | **简体中文**

# chat

聊天协议，以及所有 Rust 单元共享的内容。聊天服务（提供方）与接收端是相互独立的服务，通过携带 JSON 正文的 HTTP
彼此通信；两个 crate 分别是这条通信链路的两端，第三个是该协议的 harness 插件。第四个为单元提供控制台界面，第五个收录所有单元
在文字中查找的秘密形式；所有单元遵循的约定和共享版本也放在这里。

| Crate | 目录 | 由谁导入或运行 | 提供的内容 |
|---|---|---|---|
| `bmo-chat` | `provider/` | 聊天服务 | 聊天片段类型、提供方契约、提供方路由的服务器，以及到接收端的连接 |
| `bmo-chat-client` | `client/` | 接收端和插件 | 面向所有提供方的统一客户端，以及提供方向其投递消息的路由的服务器 |
| `bmo-chat-plugin` | `plugin/` | 编码 harness | `bmo-chat` 命令（其 `mcp` 子命令为任意提供方通过 MCP 提供 harness 工具）、`bmo-chat` skill 以及 OpenAPI 文档 |
| `bmo-console-face` | `console-face/` | 自带控制台界面的服务 | 依照 bmo-ui 控制台契约的清单、包文件及其防护，以及供测试使用的、通过 bmo-ui 发布版本读取清单的功能 |
| `bmo-secret-shapes` | `secret-shapes/` | 保存或转发人或代理所写文字的单元 | 秘密值的书写形式列表，以及将这类片段替换为读取方标记的功能 |

各个库之间互不依赖。每个库各自定义线上格式，由根 package 的测试检查两端是否一致。插件只依赖 bmo-chat-client。

## 文档

- `docs/contract.md`：片段类型，例如消息、发送、拒绝、方言以及提供方的声明。
- `docs/protocol.md`：双向的路由、其请求体和响应、错误码、重试规则，以及 harness 插件的工具。
- `docs/openapi/provider.json`、`docs/openapi/receiver.json`：以 OpenAPI 3.1 文档描述的同一组路由，每端一份，由
  bmo-chat-plugin 生成，供用其他语言编写的服务使用。提供方文档在 `x-tools` 下包含 harness 工具。
- `docs/presence.md`：在线状态与输入提示，即此刻谁在房间里输入、此刻能联系到谁，以及接收端和控制台如何处理它们。
- `docs/console-face.md`：控制台界面从服务获取什么、补充什么、响应什么。
- `docs/secret-shapes.md`：秘密形式列表、读取顺序，以及读取功能接收什么、响应什么。
- `docs/conventions.md`：所有 bmo 仓库遵循的通用线上格式、API 设计以及上下游规则。
- `docs/PERSONAL-DATA.md`：绝不能进入仓库的个人数据，以及在每个文件中查找它的检查。

## Harness 插件

`bmo-chat mcp --provider <url>` 在标准输入输出上启动 MCP 服务器，提供方的令牌通过 `BMO_CHAT_TOKEN` 或 `--token`
传入。skill 位于 `plugin/skills/bmo-chat/SKILL.md`。

## 目录结构

- `provider/`、`client/`、`plugin/`、`console-face/`、`secret-shapes/`：五个 crate，各自拥有自己的 `src/` 和 `tests/`。
- `tests/`：跨整个仓库的检查，作为根 package 的测试运行。
- `shared-versions.toml`：所有 Rust 单元以同一版本、同一 feature 组合使用的注册表 crate，它们所依赖的版本，以及所有
  使用方内置的唯一 bmo-ui 发布版本（`docs/conventions.md`，「Shared versions」）。
- `testbed.toml`：bmo-testbed 测试和检查本仓库的方式。基础库在使用方的构建中被构建，因此不声明构建，也不声明部署。

## 命令

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets
```
