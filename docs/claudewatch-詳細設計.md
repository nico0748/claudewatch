# claudewatch — 詳細設計書（Low-Level Design）

**版数:** 0.1
**作成日:** 2026-07-04
**関連文書:** [[claude-usage-monitor-要件定義]] / [[claudewatch-全体設計]]

本書は各モジュールのインターフェース、データ構造、アルゴリズム、エラー処理を定義する。言語は Rust（コア）/ TypeScript（UI）を前提とする。型表記は実装イメージであり、細部は実装時に調整する。

---

## 1. ドメインモデル

### 1.1 型定義（Rust）

```rust
/// プラン種別
enum Plan { Pro, Max }

/// 対応ブラウザ
enum Browser { Chrome, Brave, Firefox, Safari }

/// アカウント状態
enum AccountStatus { Available, InWindow, Limited }

/// 取得状態（横断）
enum FetchStatus { Ok, Stale, AuthRequired, Error }

/// 使用枠（5時間 or 週次）
struct Window {
    kind: WindowKind,          // FiveHour | Weekly | WeeklySonnet
    usage_percent: f32,        // 0.0–100.0
    resets_at: DateTime<Tz>,   // リセット時刻（取得 or 計算）
    resets_at_source: Source,  // Fetched | Computed
}
enum WindowKind { FiveHour, Weekly, WeeklySonnet }
enum Source { Fetched, Computed }

/// アカウント（最大4件）
struct Account {
    id: Uuid,
    label: String,
    plan: Plan,
    browser: Browser,
    browser_profile: String,        // プロファイル識別子
    timezone: Tz,
    weekly_reset_weekday: u8,        // 0–6
    weekly_reset_time: NaiveTime,
    windows: Vec<Window>,            // Pro:2(5h,weekly) / Max:3
    status: AccountStatus,
    fetch_status: FetchStatus,
    last_fetched_at: Option<DateTime<Utc>>,
    window_started_at: Option<DateTime<Utc>>, // 5h枠フォールバック計算用
}

/// 通知設定
struct NotificationSettings {
    reset_advance_minutes: HashMap<WindowKind, u32>, // 枠別・既定15
    notify_on_free: bool,
    usage_threshold_percent: f32,   // 既定80
    cooldown_minutes: u32,          // 既定30
    quiet_hours: Option<(NaiveTime, NaiveTime)>,
    per_window_enabled: HashMap<WindowKind, bool>,
}

/// アプリ設定
struct AppConfig {
    accounts: Vec<Account>,          // 最大4
    notifications: NotificationSettings,
    polling_interval_secs: u64,      // 既定300
    launch_at_login: bool,
    badge_mode: BadgeMode,           // NextReset | AvailableCount
}
enum BadgeMode { NextReset, AvailableCount }
```

### 1.2 永続化ポリシー

- `AppConfig`（クッキー値を除く）を SQLite / JSON に保存。
- **クッキー値・復号鍵はいかなる場合も永続化しない**（メモリ上のみ）。
- 履歴（Post-MVP）は `usage_history(account_id, window_kind, ts, usage_percent)` テーブル。

---

## 2. Cookie Provider（ブラウザ抽象層）

### 2.1 トレイト

```rust
trait CookieProvider {
    /// インストール検出とプロファイル列挙
    fn detect_profiles(&self) -> Result<Vec<ProfileInfo>, CookieError>;
    /// 指定プロファイルから claude.ai セッションクッキーを取得
    fn read_claude_cookies(&self, profile: &str) -> Result<CookieBundle, CookieError>;
}

struct ProfileInfo { id: String, display_name: String, path: PathBuf }

struct CookieBundle {
    // claude.ai の認証に必要な cookie 群（例: sessionKey 等）
    cookies: Vec<(String /*name*/, String /*value*/)>,
    domain: String,          // ".claude.ai"
    read_at: Instant,
}

enum CookieError {
    BrowserNotInstalled,
    ProfileNotFound,
    CookieStoreLocked,       // ブラウザ起動中のDBロック等
    DecryptKeyUnavailable,   // キーチェーン許可拒否等
    DecryptFailed,
    CookieMissing,           // 未ログイン
    PermissionDenied,        // TCC/FDA 不足（Safari）
    Io(String),
}
```

各ブラウザ実装：`ChromeProvider` / `BraveProvider`（共通ロジック）/ `FirefoxProvider` / `SafariProvider`。

### 2.2 Chrome / Brave 実装

保存場所（macOS 例）：
- Chrome: `~/Library/Application Support/Google/Chrome/<Profile>/Cookies`
- Brave: `~/Library/Application Support/BraveSoftware/Brave-Browser/<Profile>/Cookies`
- Linux: `~/.config/google-chrome/<Profile>/Cookies` 等

手順：
1. `Cookies` SQLite を **読み取り専用 + immutable コピー**で開く（起動中ロック回避のため一時コピー推奨）。
2. `SELECT host_key, name, encrypted_value FROM cookies WHERE host_key LIKE '%claude.ai'`。
3. 復号鍵取得：
   - macOS: `keyring` で service=`"Chrome Safe Storage"`（Braveは`"Brave Safe Storage"`）から取得 → PBKDF2(SHA1, salt=`"saltysalt"`, iter=1003, len=16) で AES鍵導出。
   - Linux: Secret Service から `Chrome Safe Storage`、取得不可なら既定パスフレーズ `"peanuts"` → 同PBKDF2（iter=1）。
4. `encrypted_value` の先頭バージョンプレフィックス（`v10`/`v11`）を判定し AES-128-CBC（IV=16×0x20）で復号。新方式は AES-256-GCM のケースもあるためバージョン分岐で対応。
5. 復号後の平文を `CookieBundle` に格納。

> DBロック対策：対象ファイルを一時ディレクトリへコピーしてから読む。WAL の場合は `-wal`/`-shm` も併せてコピー。

### 2.3 Firefox 実装

- プロファイル：`~/.mozilla/firefox/profiles.ini`（Linux）/ `~/Library/Application Support/Firefox/profiles.ini`（macOS）をパースし `Default=1` or `Path` を特定。
- `cookies.sqlite` を読み取り専用コピーで開く。
- `SELECT host, name, value FROM moz_cookies WHERE host LIKE '%claude.ai'`。値は平文のためそのまま格納。

### 2.4 Safari 実装（macOS 限定）

- 保存場所：`~/Library/Cookies/Cookies.binarycookies` および `~/Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies`。
- 独自バイナリ形式（ビッグエンディアン、page/cookie オフセットテーブル）をパース。domain に `claude.ai` を含む cookie を抽出。
- **フルディスクアクセス（FDA）** が必要な場合、`PermissionDenied` を返し UI で許可導線を提示。
- Linux ビルドでは `SafariProvider` を除外（`#[cfg(target_os = "macos")]`）。

### 2.5 ファクトリ

```rust
fn provider_for(browser: Browser) -> Box<dyn CookieProvider>
```

---

## 3. Claude API Client

### 3.1 インターフェース

```rust
struct ClaudeClient { http: reqwest::Client }

impl ClaudeClient {
    /// クッキーを付与して使用量スナップショットを取得
    async fn fetch_usage(&self, cookies: &CookieBundle)
        -> Result<UsageSnapshot, ApiError>;
}

struct UsageSnapshot {
    windows: Vec<Window>,        // 取得できた枠
    fetched_at: DateTime<Utc>,
}

enum ApiError {
    Unauthorized,     // 401/403 → AuthRequired へ
    RateLimited,      // 429 → バックオフ
    Server(u16),      // 5xx → error
    Network(String),
    Parse(String),    // スキーマ変更等
}
```

### 3.2 リクエスト仕様（要実機確定）

- ベース URL：`https://claude.ai/`。
- 認証：`Cookie` ヘッダに `CookieBundle` を連結。`User-Agent` は一般的なブラウザ相当を設定。
- エンドポイント：組織/使用量を返す内部 API（例：`/api/organizations` → org_id 取得 → 使用量エンドポイント）。**具体パスとレスポンス構造は実機調査で確定**し、パーサをスキーマ変更に強い設計（欠損許容・バージョン検知）とする。
- レート配慮：5分間隔・アカウント逐次処理。連続失敗時はバックオフ。

### 3.3 パーサ方針

- レスポンス JSON から `usage_percent` 相当・各枠 `resets_at` を抽出。
- フィールド欠損・型不一致は `Parse` エラーにせず可能な範囲で部分適用し、欠けた枠は Computed（計算）で補完。
- スキーマ変更検知時は `tracing::warn` を出し、UIに「取得仕様変更の可能性」を控えめに提示。

---

## 4. Scheduler

### 4.1 ロジック

```rust
struct Scheduler { interval: Duration /*300s*/, jitter: Duration }

// 疑似コード
loop {
    let tick = now();
    for account in accounts { spawn(refresh_account(account)); } // 逐次 or 低並列
    sleep(interval + rand_jitter());  // レート平準化のため小さなjitter
}
```

### 4.2 バックオフ

- アカウント単位で失敗回数を保持。指数バックオフ：`min(interval * 2^n, 60min)`。
- 成功で回数リセット。
- `RateLimited(429)` は即バックオフ強化。
- 動的短縮（Post-MVP）：`min_reset_at - now < 20min` のアカウントは次tickを1分間隔に短縮。

### 4.3 リフレッシュ処理

```
refresh_account(acc):
  bundle = CookieProvider(acc.browser).read_claude_cookies(acc.profile)
      err CookieMissing/DecryptKeyUnavailable/Permission → set AuthRequired; emit; return
  snap = ClaudeClient.fetch_usage(bundle)
      err Unauthorized → AuthRequired
      err RateLimited/Server/Network → set Stale(前回値保持) + backoff
      err Parse → set Stale + warn
  StateEngine.apply(acc, snap)   // 状態・枠更新, 遷移検出
  NotificationManager.evaluate(acc, transitions)
  emit account_updated(acc)
```

---

## 5. State Engine（枠計算・状態遷移）

### 5.1 リセット時刻計算（フォールバック）

- **5時間枠**：`resets_at = window_started_at + 5h`。`window_started_at` は取得値優先、なければ「limited→in_window 遷移」や最初の使用検出時刻を起点に推定。
- **週次枠**：`weekly_reset_weekday` / `weekly_reset_time` / `timezone` から次回該当日時を算出。DST 跨ぎは TZ ライブラリ（`chrono-tz`）で正規化。
- 取得値（Fetched）がある場合は常に取得値を優先し、`resets_at_source = Fetched`。

### 5.2 状態判定

```
if 全枠 usage_percent < 100 かつ アクティブ枠なし → Available
if いずれかの枠が消化中(0<usage<100, 5h枠 started) → InWindow
if いずれかの枠 usage_percent >= 100 (上限) → Limited
```

### 5.3 遷移検出

- 直前状態と比較し `Transition { from, to }` を生成。
- 重要遷移：`* → Available`（空き通知）、`Available/InWindow → Limited`。
- リセット到達（`resets_at` 経過）で使用量を 0 に、Limited/InWindow → Available へ。

---

## 6. Notification Manager

### 6.1 評価

```rust
fn evaluate(acc: &Account, transitions: &[Transition], now: DateTime) {
    // 1. 空きアカウント
    if transitioned_to(Available) && settings.notify_on_free { queue(FreeAccount, acc); }
    // 2. リセット予告（枠別）
    for w in acc.windows {
        if settings.per_window_enabled[w.kind]
           && (w.resets_at - now) <= adv(w.kind)
           && !already_notified(acc, w, "advance") {
            queue(ResetAdvance, acc, w);
        }
    }
    // 3. 上限接近
    for w in acc.windows {
        if w.usage_percent >= settings.usage_threshold_percent
           && !already_notified(acc, w, "threshold") { queue(UsageThreshold, acc, w); }
    }
    // 4. 要再ログイン
    if transitioned_to_fetch(AuthRequired) { queue(AuthRequired, acc); }
}
```

### 6.2 抑制ルール

- **クールダウン**：同一 (account, kind) は `cooldown_minutes` 以内は再通知しない。
- **静音時間帯**：`quiet_hours` 内は通知を保留（キューに積み、明けたら重要度の高い1件のみ発火 or 破棄）。
- **重複防止**：予告・閾値通知は「1リセットサイクルにつき1回」フラグで管理し、リセット到達でフラグクリア。

### 6.3 送信

- Tauri notification API（macOS 通知センター）、Linux は `notify-rust`（libnotify）。
- 通知本文例：`「個人Max」5時間枠があと15分でリセット（14:05）` / `「仕事Pro」が使えるようになりました`。

---

## 7. IPC（Tauri commands / events）

### 7.1 Commands（Front → Rust）

| command | 引数 | 戻り値 | 用途 |
|---------|------|--------|------|
| `get_accounts` | – | `Vec<AccountView>` | 一覧取得 |
| `add_account` | `NewAccount` | `AccountView` | アカウント追加 |
| `update_account` | `AccountPatch` | `AccountView` | 編集 |
| `remove_account` | `id` | `()` | 削除 |
| `detect_browser_profiles` | `browser` | `Vec<ProfileInfo>` | プロファイル検出 |
| `refresh_now` | `id?` | `()` | 手動更新 |
| `get_settings` / `update_settings` | `NotificationSettings` | – | 設定 |
| `open_claude` | `id` | `()` | 既定ブラウザで claude.ai を開く |

### 7.2 Events（Rust → Front）

| event | ペイロード | 用途 |
|-------|-----------|------|
| `account_updated` | `AccountView` | 一覧・バッジ更新 |
| `fetch_error` | `{id, kind}` | エラー表示 |
| `auth_required` | `{id}` | 要再ログイン表示 |
| `notification_sent` | `{id, kind}` | UI 履歴 |

`AccountView` は UI 表示用に整形した DTO（残り時間・状態ラベル・枠ごとの % とリセット時刻を含む。機微値は含まない）。

---

## 8. UI 詳細

### 8.1 画面

1. **ポップオーバー（メイン）**：ヘッダに「利用可: N/4」。各アカウント行＝ラベル / 状態バッジ / 枠ごとの残り時間バー（5h・週次）。行アクションに「Claudeを開く」「今すぐ更新」。
2. **アカウント追加/編集**：ラベル、プラン、ブラウザ選択（検出済み一覧）、プロファイル選択、TZ、週次リセット曜日/時刻。
3. **通知設定**：予告分数（枠別）、空き通知、上限閾値、クールダウン、静音時間帯、バッジ表示モード。
4. **権限ガイド**：Chrome/Brave キーチェーン許可、Safari FDA 許可の手順。

### 8.2 バッジ

- `NextReset`：直近リセットまでの残り時間（例 `0:14`）。
- `AvailableCount`：利用可アカウント数（例 `3`）。

---

## 9. エラー処理・回復

| 事象 | 検出 | UI表示 | 回復 |
|------|------|--------|------|
| 未ログイン/失効 | CookieMissing / 401 | 「要再ログイン」バッジ | ブラウザで再ログイン→次tickで自動復帰 |
| キーチェーン拒否 | DecryptKeyUnavailable | 「復号権限が必要」＋許可導線 | 許可後に再試行 |
| Safari FDA不足 | PermissionDenied | FDA許可ガイド | 許可後に再試行 |
| DBロック | CookieStoreLocked | 一時的取得失敗（stale） | コピー方式で回避、次tick再試行 |
| レート制限 | 429 | stale 表示 | 指数バックオフ |
| スキーマ変更 | Parse | 「取得仕様変更の可能性」控えめ表示 | 計算フォールバック継続、警告ログ |
| ネットワーク断 | Network | stale＋オフライン印 | 計算表示継続、復帰後更新 |

原則：**エラーはアカウント単位で隔離**し、他アカウントの表示・通知に波及させない。

---

## 10. セキュリティ設計

- クッキー値・AES鍵はスタック/ヒープ上のみで扱い、使用後速やかに破棄（`zeroize` 検討）。
- 永続化するのは参照情報（browser 種別・profile パス）と設定のみ。
- ネットワーク送信先は `claude.ai` に限定（allowlist）。
- ログはクッキー・トークン・鍵をマスキング。`tracing` の redaction フィルタを適用。
- 設定ファイル権限は 600 相当。

---

## 11. テスト方針

- **単体**：各 CookieProvider（テスト用ダミーDB/binarycookies fixture）、PBKDF2/AES 復号、週次リセット計算（DST 跨ぎケース）、状態遷移表、通知抑制ロジック。
- **統合**：モックHTTPサーバで UsageSnapshot パース、Scheduler のバックオフ、フォールバック計算。
- **手動/E2E**：実ブラウザでのクッキー取得（各OS/各ブラウザ）、通知発火、権限ダイアログ挙動。
- **回帰**：内部エンドポイントのスキーマ差分検知テスト（サンプルレスポンス固定）。

---

## 12. 実装順序（詳細）

1. ドメイン型・Config永続化・SQLite スキーマ。
2. `FirefoxProvider`（最も単純）→ Provider トレイト確立。
3. `ChromeProvider`/`BraveProvider`（復号）→ `keyring` 連携。
4. `SafariProvider`（binarycookies パーサ、macOS）。
5. `ClaudeClient`（実機でエンドポイント確定・パーサ）。
6. `StateEngine`（計算・遷移）。
7. `Scheduler`（ポーリング・バックオフ）。
8. `NotificationManager`（抑制・静音）。
9. IPC・フロントUI（ポップオーバー→設定→権限ガイド）。
10. パッケージング（.app 署名・notarize / Linux AppImage）。
