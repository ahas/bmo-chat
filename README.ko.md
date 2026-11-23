[English](README.md) | **한국어** | [日本語](README.ja.md) | [简体中文](README.zh-CN.md)

# chat

채팅 프로토콜과 모든 Rust 유닛이 공유하는 것들을 담은 저장소입니다. 채팅 서비스(프로바이더)와 수신 측은 서로
분리된 서비스이며, JSON 본문을 담은 HTTP로 서로 통신합니다. 두 크레이트가 그 연결의 양 끝이고, 세 번째 크레이트는
이 프로토콜의 하네스 플러그인입니다. 네 번째 크레이트는 유닛의 콘솔 화면을 제공하며, 모든 유닛이 따르는 규약과
공유 버전도 이곳에 있습니다.

| 크레이트 | 디렉터리 | 가져다 쓰거나 실행하는 쪽 | 제공하는 것 |
|---|---|---|---|
| `bmo-chat` | `provider/` | 채팅 서비스 | 채팅 조각 타입, 프로바이더 계약, 프로바이더 라우트용 서버, 수신 측으로의 연결 |
| `bmo-chat-client` | `client/` | 수신 측과 플러그인 | 모든 프로바이더에 대한 하나의 클라이언트, 프로바이더가 메시지를 넘기는 라우트용 서버 |
| `bmo-chat-plugin` | `plugin/` | 코딩 하네스 | `bmo-chat` 명령(`mcp` 서브커맨드가 어떤 프로바이더에 대해서든 하네스 도구를 MCP로 제공), `bmo-chat` 스킬, OpenAPI 문서 |
| `bmo-console-face` | `console-face/` | 콘솔 화면을 가져오는 서비스 | bmo-ui의 콘솔 계약에 따른 매니페스트, 번들 파일과 이를 지키는 가드, 테스트용으로 bmo-ui 릴리스를 통해 매니페스트를 읽는 기능 |

라이브러리끼리는 서로 의존하지 않습니다. 각 라이브러리가 와이어 형식을 스스로 정의하고, 루트 패키지의 테스트가
양쪽이 일치하는지 검사합니다. 플러그인은 bmo-chat-client에만 의존합니다.

## 문서

- `docs/contract.md`: 메시지, 전송, 거부, 방언, 프로바이더 선언 같은 조각 타입.
- `docs/protocol.md`: 양방향 라우트, 그 본문과 응답, 오류 코드, 재시도 규칙, 하네스 플러그인의 도구.
- `docs/openapi/provider.json`, `docs/openapi/receiver.json`: 같은 라우트를 양쪽 각각에 대해 OpenAPI 3.1 문서로
  옮긴 것으로, bmo-chat-plugin이 생성하며 다른 언어로 작성하는 서비스를 위한 것입니다. 프로바이더 문서는 하네스
  도구를 `x-tools` 아래에 담고 있습니다.
- `docs/presence.md`: 접속 상태와 입력 중 표시, 즉 지금 방에서 누가 글을 쓰고 있는지와 지금 누구에게 닿을 수
  있는지, 그리고 수신 측과 콘솔이 이를 어떻게 다루는지.
- `docs/console-face.md`: 콘솔 화면이 서비스에서 무엇을 받고, 무엇을 채우며, 무엇을 응답하는지.
- `docs/conventions.md`: 모든 bmo 저장소가 따르는 공통 와이어 형식, API 설계, 업스트림과 다운스트림 규칙.
- `docs/PERSONAL-DATA.md`: 저장소에 절대 들어가면 안 되는 개인정보와, 모든 파일에서 이를 찾아내는 검사.

## 하네스 플러그인

`bmo-chat mcp --provider <url>`은 표준 입출력으로 MCP 서버를 시작하며, 프로바이더의 토큰은 `BMO_CHAT_TOKEN`
또는 `--token`으로 받습니다. 스킬은 `plugin/skills/bmo-chat/SKILL.md`입니다.

## 구성

- `provider/`, `client/`, `plugin/`, `console-face/`: 네 개의 크레이트로, 각각 자체 `src/`와 `tests/`를 가집니다.
- `tests/`: 저장소 전체에 걸친 검사로, 루트 패키지의 테스트로 실행됩니다.
- `shared-versions.toml`: 모든 Rust 유닛이 하나의 버전과 하나의 기능 조합으로 가져다 쓰는 레지스트리 크레이트,
  이들이 기반으로 삼는 버전, 그리고 모든 사용처가 들여오는 bmo-ui의 단일 릴리스(`docs/conventions.md`,
  「Shared versions」).
- `testbed.toml`: bmo-testbed가 이 저장소를 테스트하고 검사하는 방법. 기반 라이브러리는 사용처의 빌드 안에서
  빌드되므로 빌드도 배포도 선언하지 않습니다.

## 명령

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets
```
