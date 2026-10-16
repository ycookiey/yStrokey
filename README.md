# yStrokey

Windows向けキー入力視覚化（OSD）ツール。

## インストール

[Scoop](https://scoop.sh/) 経由:

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install ystrokey
```

## ビルド

必要環境: Windows 10/11, Rust toolchain (`stable-x86_64-pc-windows-msvc`)

```bash
cargo build --release
./target/release/ystrokey.exe
```

## 設定

初回起動時に `config.json` が自動生成される。編集すればホットリロードで即反映。設定ファイルの場所は`%APPDATA%\ystrokey\config.json`、ログの場所は`%LOCALAPPDATA%\ystrokey\logs\`。

主な設定項目:

| キー | 説明 | デフォルト |
|------|------|-----------|
| `display.position` | 表示位置 | `bottom-center` |
| `display.max_items` | 最大同時表示数 | `5` |
| `display.display_duration_ms` | 表示時間 (ms) | `2000` |
| `behavior.group_timeout_ms` | 連続入力グルーピング閾値 (ms)。0で無効 | `300` |
| `behavior.max_group_size` | 1グループの最大キー数 | `10` |
| `behavior.pause_in_games` | ゲームが前面にある間、OSDの表示とキー入力の取得を止める | `true` |
| `hotkey.toggle` | OSD切替ホットキー | `Ctrl+Alt+F12` |

## ゲーム中の動作

Windowsがゲームとして記録しているアプリが前面にある間は、OSDの表示とキー入力の取得を止める。ゲームの記録はWindowsがXbox Game Barのために持つもので、ゲームのランチャーが含まれることもある。ゲーム中も表示するには、`behavior.pause_in_games`を`false`にするか、設定ウィンドウの**Pause while a game is in the foreground**のチェックを外す。