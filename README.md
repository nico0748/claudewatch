# claudewatch

Menu-bar / tray app that monitors usage limits and reset times across multiple Claude accounts, with advance reset alerts and free-account notifications.

複数の Claude アカウント（最大4）の使用量とリセット時刻を、各ブラウザのログイン済みセッションから取得して一元監視する常駐アプリです。5時間ローリング枠と週次枠を可視化し、リセット予告・空きアカウント通知・上限接近アラートを行います。

> ⚠️ **状態:** 開発初期（スキャフォールド段階）。内部エンドポイントのスキーマ確定など未実装項目があります。詳細は [`docs/lld.md`](docs/lld.md) を参照。

## 特徴

- 最大4アカウントの使用量・リセット時刻を一元表示（メニューバー / システムトレイ常駐）
- 各ブラウザ（Chrome / Brave / Firefox / Safari）のクッキーストアからセッションを自動取得
- 5分間隔ポーリング（指数バックオフ付き）
- リセット予告 / 空きアカウント復帰 / 上限接近 / 要再ログイン の各通知
- クッキー値は永続化せず都度読み出し。参照情報のみ保存

## 対応環境

| OS | 常駐UI | 対応ブラウザ |
|----|--------|--------------|
| macOS | メニューバー | Chrome / Brave / Firefox / Safari |
| Linux | システムトレイ | Chrome / Brave / Firefox |

Windows は将来対応（現状スコープ外）。

## 技術スタック

Tauri 2.x（Rust コア + WebView フロント） / TypeScript(Svelte) / SQLite / keyring。

## 開発

### 前提

- Rust (stable) + `cargo`
- Node.js 20+ / npm
- Tauri CLI: `cargo install tauri-cli --version "^2.0"`

### セットアップ

```bash
npm install
cargo tauri dev      # 開発起動
cargo tauri build    # 配布ビルド (.app / AppImage)
```

## ディレクトリ構成

```
claudewatch/
├─ docs/                 要件定義・設計書
│  ├─ requirements.md    要件定義書
│  ├─ hld.md             全体設計書
│  └─ lld.md             詳細設計書
├─ src-tauri/            Rust コア（Tauri アプリ）
│  ├─ src/
│  │  ├─ domain/         ドメインモデル・状態遷移・枠計算
│  │  ├─ acquisition/    データ取得（cookie provider + claude client）
│  │  ├─ scheduler/      ポーリング・バックオフ
│  │  ├─ notification/   通知マネージャ
│  │  ├─ storage/        設定永続化・セキュアストレージ
│  │  └─ ipc/            Tauri commands / events
│  ├─ Cargo.toml
│  └─ tauri.conf.json
├─ src/                  フロントエンド (TS / Svelte)
├─ tests/                統合テスト
└─ README.md
```

## セキュリティ / プライバシー

すべての処理はローカル完結し、通信先は `claude.ai` に限定します。ブラウザのクッキー値・復号鍵はメモリ上でのみ扱い永続化しません。詳細は [`docs/lld.md` §10](docs/lld.md)。

本ツールは Claude の利用規約・自動化ポリシーの精査を前提に開発されています。利用は各自の責任で行ってください。

## ライセンス

[MIT](LICENSE)
