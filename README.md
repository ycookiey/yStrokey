# yStrokey

キー入力を画面に表示する、Windows向けのオンスクリーンディスプレイ(OSD)ツール。

## インストール

[Scoop](https://scoop.sh/)でインストールする。

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install ystrokey
```

## ビルド

必要な環境は、Windows 10/11とRustのツールチェーン(`stable-x86_64-pc-windows-msvc`)。

```bash
cargo build --release
./target/release/ystrokey.exe
```

## 設定

初回起動時に`config.json`を自動で生成する。`config.json`を編集して保存すると、再起動せずに設定が反映される。

`config.json`は`%APPDATA%\ystrokey\config.json`に置く。環境変数`APPDATA`が空のときは、実行ファイルと同じフォルダに置く。ログは`%LOCALAPPDATA%\ystrokey\logs\`に出力する。

主な設定項目:

| キー | 説明 | デフォルト |
|------|------|-----------|
| `display.position` | OSDを表示する位置 | `bottom-center` |
| `display.max_items` | 同時に表示する項目の最大数 | `5` |
| `display.display_duration_ms` | 項目を表示する時間 (ms) | `2000` |
| `behavior.group_timeout_ms` | 連続入力を1グループにまとめる入力間隔の上限 (ms)。0でグループ化を無効にする | `300` |
| `behavior.max_group_size` | 1グループにまとめるキーの最大数 | `10` |
| `behavior.pause_in_games` | ゲームが前面にある間、OSDの表示とキー入力の取得を止める | `true` |
| `hotkey.toggle` | OSDの表示と非表示を切り替えるホットキー | `Ctrl+Alt+F12` |

## ゲーム中の動作

Windowsがゲームとして記録しているアプリが前面にある間は、OSDの表示とキー入力の取得を止める。Windowsは、Xbox Game Barのためにゲームの実行ファイルの一覧を記録している。一覧にはゲームのランチャーが含まれることもある。一覧を読めないときは、どのアプリもゲームとみなさず、OSDを止めない。

ゲーム中もOSDを表示するには、`behavior.pause_in_games`を`false`にするか、設定ウィンドウの「Pause while a game is in the foreground」のチェックを外す。
