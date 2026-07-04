# claudewatch — 全体設計書（High-Level Design）

**版数:** 0.1
**作成日:** 2026-07-04
**対象アプリ:** claudewatch（複数 Claude アカウントの使用量・リセット時刻モニター）
**関連文書:** [[claude-usage-monitor-要件定義]]

---

## 0. リポジトリ情報

| 項目 | 内容 |
|------|------|
| **リポジトリ名** | `claudewatch` |
| **Short description** | Menu-bar / tray app that monitors usage limits and reset times across multiple Claude accounts, with advance reset alerts and free-account notifications. |
| **日本語説明** | 複数の Claude アカウント（最大4）の使用量とリセット時刻を、各ブラウザのセッションから取得して一元監視する常駐アプリ。リセット予告・空きアカウント通知に対応。 |
| **Topics（タグ）** | `claude`, `tauri`, `rust`, `menubar`, `system-tray`, `usage-monitor`, `macos`, `linux`, `desktop-app` |
| **ライセンス（案）** | MIT（要確認） |
| **代替名候補** | `claude-gauge`, `claude-quota-radar`, `claude-reset-watch` |

---

## 1. 目的とスコープ

複数の Claude アカウントの二重制限（5時間ローリング枠／週次固定枠）と使用量を、各ブラウザのログイン済みセッションから自動取得し、常駐アプリで一元可視化する。リセットの事前予告と、使えるアカウントへの復帰通知により「待ち時間ゼロで作業継続」を実現する。

- 対象OS：macOS（メニューバー）、Linux（システムトレイ）
- 対象アカウント：最大4
- データ取得：方式A（各ブラウザのクッキーストアからセッションを読み出し内部エンドポイントへリクエスト）
- ポーリング：5分間隔

詳細な機能要件・非機能要件は要件定義書を参照。本書はアーキテクチャと全体構成を定義する。

---

## 2. アーキテクチャ概要

### 2.1 レイヤー構成

```
┌────────────────────────────────────────────────────────────┐
│  Presentation Layer  (WebView: フロントUI)                  │
│   ・トレイ/メニューバー ポップオーバー                       │
│   ・アカウント設定画面 / 通知設定画面                        │
│   ・アイコンバッジ表示制御                                   │
└───────────────▲───────────────────────┬────────────────────┘
                │ IPC (Tauri commands /   │ events
                │      event emit)         ▼
┌───────────────┴────────────────────────────────────────────┐
│  Application Layer  (Rust: コア)                            │
│   ┌──────────────┐ ┌───────────────┐ ┌──────────────────┐  │
│   │ Scheduler     │ │ Domain/State  │ │ Notification      │  │
│   │ (5分ポーリング)│→│ (枠計算・判定) │→│ Manager           │  │
│   └──────┬───────┘ └──────▲────────┘ └──────────────────┘  │
│          ▼                │                                  │
│   ┌──────────────────────┴───────────────────────────────┐ │
│   │ Data Acquisition (方式A)                              │ │
│   │  ┌────────────────┐   ┌───────────────────────────┐  │ │
│   │  │ Cookie Provider │──▶│ Claude API Client          │  │ │
│   │  │ (ブラウザ抽象層) │   │ (内部エンドポイント)         │  │ │
│   │  └────────────────┘   └───────────────────────────┘  │ │
│   └───────────────────────────────────────────────────────┘ │
└───────────────┬────────────────────────────────────────────┘
                ▼
┌────────────────────────────────────────────────────────────┐
│  Infrastructure Layer                                       │
│   ・Secure Storage（Keychain / Secret Service）             │
│   ・Config Store（設定・アカウント参照情報）                 │
│   ・OS Notification / Tray API                              │
│   ・Browser Cookie Stores（読み取り対象）                    │
└────────────────────────────────────────────────────────────┘
```

### 2.2 主要モジュール一覧

| モジュール | 責務 | 実装層 |
|-----------|------|--------|
| **Cookie Provider** | 各ブラウザのクッキーストアから claude.ai セッションを読み出す抽象層。ブラウザごとの実装を内包 | Rust |
| **Claude API Client** | 取得クッキーを用いて内部エンドポイントへリクエストし、使用量・リセット時刻を取得・パース | Rust |
| **Scheduler** | 5分間隔のポーリング、バックオフ、動的間隔制御 | Rust |
| **Domain / State Engine** | 枠のリセット計算、状態判定（available / in_window / limited）、状態遷移の検出 | Rust |
| **Notification Manager** | リセット予告・空き通知・上限接近アラートの発火、クールダウン、静音時間帯 | Rust |
| **Config Store** | アカウント参照情報・通知設定の永続化 | Rust |
| **Secure Storage Adapter** | OSキーチェーン連携（Chrome/Brave復号鍵の取得、機微値の保管） | Rust |
| **Tray / Menubar Controller** | 常駐アイコン・バッジ・ポップオーバー制御 | Rust + Web |
| **UI (Frontend)** | 一覧・設定画面の描画、ユーザー操作 | Web (TS) |

---

## 3. 技術スタック

| 領域 | 採用技術 | 備考 |
|------|---------|------|
| アプリ基盤 | **Tauri 2.x**（Rust + WebView） | 省メモリ常駐、トレイ/メニューバー、通知プラグイン |
| コアロジック | **Rust** | データ取得・計算・スケジューラ |
| フロントエンド | **TypeScript + 軽量フレームワーク**（Svelte 推奨 / 代替 React） | バンドル小・高速。最終判断は実装時 |
| 状態管理(前) | フロント内ローカルステート | UIは薄く保つ |
| DB / 永続化 | **SQLite**（`rusqlite`）＋設定JSON | 履歴・設定。機微値は保存しない |
| セキュアストレージ | `keyring` クレート（Keychain / Secret Service） | Chrome/Brave復号鍵取得・参照情報保管 |
| HTTP | `reqwest`（rustls） | 内部エンドポイントアクセス |
| クッキー復号 | ブラウザ別（下記） | 抽象トレイト背後に実装 |
| 通知 | Tauri notification / `notify-rust`（Linux） | OSネイティブ通知 |
| スケジューラ | `tokio` タイマ | 非同期ポーリング |
| ロギング | `tracing` | 機微情報マスキング |

### 3.1 ブラウザ別クッキー取得ライブラリ方針

| ブラウザ | 保存形式 | 実装方針 |
|----------|----------|----------|
| Chrome / Brave | SQLite `Cookies` + AES暗号化値 | `rusqlite` で読み取り、`keyring` で "Chrome/Brave Safe Storage" 鍵取得 → AES-256-GCM/CBC 復号（`aes-gcm`/`aes`）。Linux は Secret Service、非対応時は既定パスフレーズ `peanuts` フォールバック |
| Firefox | SQLite `cookies.sqlite`（平文） | `rusqlite` で直接読み取り。プロファイル特定は `profiles.ini` パース |
| Safari | `Cookies.binarycookies`（独自バイナリ, macOS限定） | 自作パーサ or 既存クレート。TCC フルディスクアクセス許可が必要な場合あり |

---

## 4. データフロー

### 4.1 ポーリングによる更新フロー（5分ごと）

```
Scheduler(5分tick)
  └─ for each Account (最大4):
       1. Cookie Provider: 対象ブラウザ/プロファイルから claude.ai クッキー読み出し
            └─ 失効/欠落 → fetch_status=auth_required, 「要再ログイン」表示
       2. Claude API Client: クッキーで内部エンドポイントへGET
            └─ 4xx/認証エラー → auth_required
            └─ ネットワーク/5xx → error（バックオフ、前回値をstale表示）
            └─ 成功 → 使用量 %・枠状態・リセット時刻をパース
       3. State Engine: 取得値で Account 状態更新、リセット時刻確定
            └─ 取得不可時はローカル計算（起点+5h / 週次固定）でフォールバック
       4. 状態遷移を検出 → Notification Manager へイベント
       5. UI へ event emit（一覧・バッジ更新）
```

### 4.2 通知トリガ

| 通知 | トリガ条件 |
|------|-----------|
| リセット予告 | `reset_at - now <= reset_advance_minutes`（既定15分、枠別に設定可）かつ未通知 |
| 空きアカウント | `status` が limited/in_window → available へ遷移 |
| 上限接近 | `usage_percent >= threshold`（既定80%）かつ未通知 |
| 要再ログイン | `fetch_status = auth_required` へ遷移 |

各通知はクールダウンと静音時間帯を適用。

---

## 5. 状態モデル（アカウント）

```
             ┌───────────┐  枠を使い始め   ┌────────────┐
             │ available │───────────────▶│ in_window   │
             │ (利用可)   │◀───────────────│ (5h枠消化中) │
             └─────┬─────┘   リセット到達   └──────┬─────┘
                   │                               │ 上限到達
                   │ 上限到達                       ▼
                   │                        ┌────────────┐
                   └───────────────────────▶│ limited     │
                            リセット到達 ◀────│ (上限到達)   │
                                            └────────────┘

  横断状態: auth_required（クッキー失効）／ error（取得失敗, 前回値をstale表示）
```

---

## 6. ディレクトリ構成（案）

```
claudewatch/
├─ Cargo.toml                 # ワークスペース
├─ src-tauri/                 # Tauri アプリ（Rust コア）
│  ├─ src/
│  │  ├─ main.rs
│  │  ├─ app.rs               # Tauri setup, tray, commands
│  │  ├─ scheduler/           # ポーリング・バックオフ
│  │  ├─ acquisition/
│  │  │  ├─ cookie/           # ブラウザ抽象 + chrome/brave/firefox/safari
│  │  │  └─ claude_client.rs  # 内部エンドポイントクライアント
│  │  ├─ domain/              # Account, 枠計算, 状態遷移
│  │  ├─ notification/        # 通知マネージャ
│  │  ├─ storage/             # config, sqlite, keyring
│  │  └─ ipc/                 # commands / events 定義
│  ├─ tauri.conf.json
│  └─ icons/
├─ src/                       # フロントエンド (TS/Svelte)
│  ├─ routes/                 # popover, settings-accounts, settings-notify
│  ├─ lib/                    # api(IPC), stores, components
│  └─ main.ts
├─ docs/
│  ├─ requirements.md
│  ├─ hld.md                  # 本書
│  └─ lld.md                  # 詳細設計
├─ tests/                     # 統合テスト
└─ README.md
```

---

## 7. 横断的関心事

- **セキュリティ**：クッキー値はメモリ上でのみ扱い永続化しない。参照情報（ブラウザ種別・プロファイル）のみ保存。外部送信は claude.ai の内部エンドポイントのみ。ログは機微情報マスキング。
- **権限**：Chrome/Brave 復号時のキーチェーン許可、Safari のフルディスクアクセスを初回にガイド。
- **タイムゾーン**：全リセット計算はアカウントの TZ を明示保持し、DST を考慮。
- **可用性**：ネットワーク不通・取得失敗時も計算ベース表示を継続。
- **パフォーマンス**：待機時 CPU ほぼ0%、メモリ数十MB目標。ポーリングは5分間隔で軽量。
- **エラーハンドリング**：取得失敗はアカウント単位で分離し、他アカウント表示に影響させない。

---

## 8. マイルストーン（実装ロードマップ案）

1. **M1 基盤**：Tauri 雛形、トレイ/メニューバー常駐、設定永続化、ダミーデータで一覧表示。
2. **M2 クッキー取得**：Firefox（平文）→ Chrome/Brave（復号）→ Safari の順にブラウザ抽象層を実装。
3. **M3 データ取得**：Claude API Client、内部エンドポイントのパース、State Engine 連携。
4. **M4 スケジューラ**：5分ポーリング、バックオフ、フォールバック計算。
5. **M5 通知**：予告・空き・上限接近・要再ログインの各通知、クールダウン/静音。
6. **M6 仕上げ**：履歴/グラフ（Post-MVP）、権限ガイドUX、パッケージング（.app / AppImage）。

---

## 9. 未確定事項（詳細設計で解消）

- 内部エンドポイントの具体的な URL・レスポンススキーマ（実機調査が必要）。
- フロントフレームワークの最終選定（Svelte / React）。
- Safari binarycookies パースの既存クレート採否。
- ライセンス確定。
