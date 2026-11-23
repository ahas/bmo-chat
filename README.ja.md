[English](README.md) | [한국어](README.ko.md) | **日本語** | [简体中文](README.zh-CN.md)

# chat

チャットプロトコルと、すべての Rust ユニットが共有するものを収めたリポジトリです。チャットサービス(プロバイダー)
と受信側は別々のサービスで、JSON ボディの HTTP で互いに通信します。二つのクレートがその通信の両端となり、三つ目は
このプロトコルのハーネスプラグインです。四つ目はユニットのコンソール画面を提供し、五つ目はすべてのユニットが文章から探す秘密の形を
収めます。すべてのユニットが従う規約と共有バージョンもここに置かれています。

| クレート | ディレクトリ | 利用・実行する側 | 提供するもの |
|---|---|---|---|
| `bmo-chat` | `provider/` | チャットサービス | チャットの部品型、プロバイダー契約、プロバイダールート用のサーバー、受信側への接続 |
| `bmo-chat-client` | `client/` | 受信側とプラグイン | すべてのプロバイダーに対応する一つのクライアント、プロバイダーがメッセージを渡すルート用のサーバー |
| `bmo-chat-plugin` | `plugin/` | コーディングハーネス | `bmo-chat` コマンド(その `mcp` サブコマンドがどのプロバイダーに対してもハーネスのツールを MCP で提供する)、`bmo-chat` スキル、OpenAPI ドキュメント |
| `bmo-console-face` | `console-face/` | コンソール画面を持ち込むサービス | bmo-ui のコンソール契約に沿ったマニフェスト、バンドルのファイルとそれを守るガード、テスト用に bmo-ui のリリースを通してマニフェストを読む機能 |
| `bmo-secret-shapes` | `secret-shapes/` | 人やエージェントが書いた文章を保存または転送するユニット | 秘密の値が書かれる形の一覧と、その部分を読み手の印に置き換える機能 |

ライブラリ同士は依存し合いません。それぞれがワイヤー形式を独自に定義し、ルートパッケージのテストが両者の一致を
確認します。プラグインは bmo-chat-client のみに依存します。

## ドキュメント

- `docs/contract.md`: メッセージ、送信、拒否、方言、プロバイダーの宣言といった部品型。
- `docs/protocol.md`: 双方向のルート、そのボディと応答、エラーコード、リトライ規則、ハーネスプラグインのツール。
- `docs/openapi/provider.json`、`docs/openapi/receiver.json`: 同じルートを両側それぞれについて OpenAPI 3.1 で
  記したドキュメントで、bmo-chat-plugin が生成し、他の言語で書かれるサービスのためにあります。プロバイダー側の
  ドキュメントはハーネスのツールを `x-tools` に持ちます。
- `docs/presence.md`: プレゼンスと入力中表示、つまり今ルームで誰が書いているか、今誰に連絡がつくか、そして受信側と
  コンソールがそれをどう扱うか。
- `docs/console-face.md`: コンソール画面がサービスから何を受け取り、何を補い、何を応答するか。
- `docs/secret-shapes.md`: 秘密の形の一覧、読む順序、そして読み取りが何を受け取り何を応答するか。
- `docs/conventions.md`: すべての bmo リポジトリが従う共通のワイヤー形式、API 設計、アップストリームと
  ダウンストリームの規則。
- `docs/PERSONAL-DATA.md`: リポジトリに決して入れてはならない個人データと、すべてのファイルからそれを探す検査。

## ハーネスプラグイン

`bmo-chat mcp --provider <url>` は標準入出力で MCP サーバーを起動します。プロバイダーのトークンは
`BMO_CHAT_TOKEN` または `--token` で渡します。スキルは `plugin/skills/bmo-chat/SKILL.md` です。

## 構成

- `provider/`、`client/`、`plugin/`、`console-face/`、`secret-shapes/`: 五つのクレートで、それぞれ独自の `src/` と `tests/` を持ちます。
- `tests/`: リポジトリ全体にまたがる検査で、ルートパッケージのテストとして実行されます。
- `shared-versions.toml`: すべての Rust ユニットが一つのバージョンと一つのフィーチャー構成で使うレジストリの
  クレート、それらが前提とするバージョン、そしてすべての利用側が取り込む bmo-ui の唯一のリリース
  (`docs/conventions.md`、「Shared versions」)。
- `testbed.toml`: bmo-testbed がこのリポジトリをテスト・検査する方法。ベースライブラリは利用側のビルドの中で
  ビルドされるため、ビルドもデプロイも宣言しません。

## コマンド

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets
```
