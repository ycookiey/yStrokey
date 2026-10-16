# yStrokey

キー入力を画面に表示する、Windows向けのオンスクリーンディスプレイ(OSD)ツール。

主な利用者は次の2種類。

- タイピングやショートカットの練習中に、押したキーが意図どおりかを画面で確認したい人
- 配信、録画、解説で、押したキーを見せたい人

## 何ができるか

- 押したキーを、画面の最前面に出る小さなOSDに表示する。OSDに並ぶ1つ1つの表示を、表示枠と呼ぶ。OSDはクリックを通すので、下のウィンドウをそのまま操作できる
- テンキーを、通常の数字キーと区別して表示する。`Num0`、`NumEnter`のように表示する。区別しない設定は`behavior.distinguish_numpad`
- キーを押している間と離した後で、表示を変える。離したときに新しい表示枠を足す表示にも変えられる。設定は`behavior.key_transition_mode`
- 同じキーの連打回数を表示する。設定は`behavior.show_repeat_count`
- 短い間隔の連続入力を、1つの表示枠にまとめる。設定は`behavior.group_timeout_ms`と`behavior.max_group_size`
- `Ctrl+C`のようなショートカットに、`Copy`などのラベルを付けて表示する。組み合わせとラベルは`shortcuts`で変更、追加できる
- IMEで入力中の未確定の文字列を表示する。設定は`behavior.show_ime_composition`
- クリップボードにコピーした文字列を表示する。文字列は`behavior.clipboard_max_chars`の文字数で切る。画像などの文字列以外は表示しない。設定は`behavior.show_clipboard`
- Caps Lock、Num Lock、Scroll Lockの状態が変わったときに、状態を表示する。設定は`behavior.show_lock_indicators`
- 表示しないキーを指定する。設定は`behavior.ignored_keys`
- 前面のウィンドウがあるモニターにOSDを移す。OSDをドラッグして動かすと、位置をモニターごとに保存する。ドラッグするには、`animation.ghost_modifier`のキーを押しながらOSDにカーソルを近づける。近づけるとOSDの背景が見え、OSDの上にカーソルを置くとドラッグできる
- OSDを画面のキャプチャや録画に映さない。設定は`behavior.exclude_from_capture`
- ホットキーでOSDの表示と非表示を切り替える。設定は`hotkey.toggle`
- タスクトレイのメニューから、表示と非表示の切り替え、自動起動の切り替え、設定ウィンドウを開く、設定のエクスポートとインポート、終了ができる

## 動作環境

Windows 11で動作を確認している。ほかのバージョンは確認していない。

## インストール

[Scoop](https://scoop.sh/)でインストールする。

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install ystrokey
```

インストール後は、コマンドの`ystrokey`を実行するか、スタートメニューの「yStrokey」を開く。起動すると、タスクトレイにアイコンが出る。

## 設定

初回起動時に`config.json`を自動で生成する。設定を変える方法は2つある。

- タスクトレイのアイコンを右クリックし、メニューの「設定」から設定ウィンドウを開く。`tray.menu_language`が`en`のときは「Settings」と表示する。設定を変えると、`config.json`に保存して反映する
- `config.json`を直接編集する。保存すると、再起動せずに設定が反映される

`config.json`は`%APPDATA%\ystrokey\config.json`に置く。環境変数`APPDATA`が無いか空のときは、実行ファイルと同じフォルダに置く。ログは`%LOCALAPPDATA%\ystrokey\logs\`に出力する。

主な設定キー:

| キー | 説明 | 既定値 |
|------|------|-----------|
| `display.position` | OSDを表示する位置。`top-left`、`top-center`、`top-right`、`bottom-left`、`bottom-center`、`bottom-right`から選ぶ | `bottom-center` |
| `display.max_items` | 同時に表示する表示枠の最大数 | `5` |
| `display.display_duration_ms` | 表示枠を表示する時間。単位はms | `2000` |
| `behavior.key_transition_mode` | キーを離したときの表示。`single-cell`は同じ表示枠の表示を変え、`split-cells`は新しい表示枠を足す | `single-cell` |
| `behavior.distinguish_numpad` | テンキーを通常のキーと区別して表示する | `true` |
| `behavior.show_repeat_count` | 同じキーの連打回数を表示する | `true` |
| `behavior.show_ime_composition` | IMEで入力中の未確定の文字列を表示する | `true` |
| `behavior.show_clipboard` | クリップボードにコピーした内容を表示する | `false` |
| `behavior.show_lock_indicators` | Caps Lock、Num Lock、Scroll Lockの状態を表示する | `true` |
| `behavior.group_timeout_ms` | 連続入力を1グループにまとめる入力間隔の上限。単位はms。0でグループ化を無効にする | `300` |
| `behavior.max_group_size` | 1グループにまとめるキーの最大数 | `10` |
| `behavior.ignored_keys` | 表示しないキーの名前の一覧 | 空 |
| `behavior.exclude_from_capture` | OSDを画面のキャプチャや録画に映さない | `false` |
| `behavior.pause_in_games` | ゲームが前面にある間、OSDの表示とキー入力の取得を止める | `true` |
| `privacy.enabled` | `privacy.blocked_apps`のアプリが前面にある間、OSDに何も表示しない | `true` |
| `privacy.blocked_apps` | 表示しないアプリの実行ファイル名の一覧 | `KeePass.exe`、`1Password.exe` |
| `hotkey.toggle` | OSDの表示と非表示を切り替えるホットキー | `Ctrl+Alt+F12` |
| `startup.autostart_enabled` | Windowsのサインイン時にyStrokeyを自動で起動する | `false` |
| `animation.ghost_modifier` | OSDをドラッグできるようにするキー。`ctrl`、`alt`、`shift`から選ぶ | `ctrl` |
| `tray.menu_language` | タスクトレイのメニューの言語。`ja`か`en` | `ja` |
| `diagnostics.level` | ログの詳細さ。`error`、`warn`、`info`、`debug`から選ぶ | `info` |

## プライバシー

`privacy.enabled`が`true`のとき、前面のアプリの実行ファイル名が`privacy.blocked_apps`のいずれかと一致する間は、OSDに何も表示しない。すでに表示している表示枠も消える。実行ファイル名は、大文字と小文字を区別せずに比べる。クリップボードにコピーした文字列は、既定ではOSDに表示しない。コピーした文字列が配信や録画に映らないようにするため。表示するには、`behavior.show_clipboard`を`true`にする。`privacy.blocked_apps`の既定は、パスワードマネージャーの`KeePass.exe`と`1Password.exe`。表示しないアプリは、設定ウィンドウか`privacy.blocked_apps`で追加、削除できる。

前面のアプリの実行ファイル名を取得できないときも、安全のためOSDに何も表示しない。

OSDを画面のキャプチャや録画に映さないようにするには、`behavior.exclude_from_capture`を`true`にする。既定は`false`で、OSDはキャプチャや録画に映る。

## ゲーム中の動作

Windowsがゲームとして記録しているアプリが前面にある間は、OSDの表示とキー入力の取得を止める。Windowsは、Xbox Game Barのためにゲームの実行ファイルの一覧を記録している。一覧にはゲームのランチャーが含まれることもある。一覧を読めないときは、どのアプリもゲームとみなさず、OSDを止めない。

yStrokeyは低レベルのキーボードフックでキー入力を取得する。ゲームが前面にある間はキーボードフックを外すので、ゲームのキー入力はyStrokeyの応答を待たない。ゲームやゲームのランチャーの操作もOSDに表示したいときは、`behavior.pause_in_games`を`false`にするか、設定ウィンドウの「Pause while a game is in the foreground」のチェックを外す。

## アンインストール

1. タスクトレイのアイコンを右クリックし、メニューの「自動起動」にチェックが付いていれば、選んで外す。自動起動を有効にすると、レジストリ`HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`に`yStrokey`という値を書き込む。メニューのチェックを外すと、`yStrokey`の値を削除する
2. タスクトレイのメニューの「終了」を選んで、yStrokeyを終了する
3. 次のコマンドでアンインストールする

   ```powershell
   scoop uninstall ystrokey
   ```

アンインストールしても、次のファイルは残る。不要なら手動で削除する。

- `%APPDATA%\ystrokey\`: 設定の`config.json`。読み込めなかった`config.json`を退避した`config.json.invalid.<数字>.json`もある
- `%LOCALAPPDATA%\ystrokey\`: ログ
- 実行ファイルと同じフォルダの`config.json`: 以前の置き場所の名残で、あるときだけ。起動時に`%APPDATA%\ystrokey\config.json`が無ければ、実行ファイルと同じフォルダの`config.json`を複製して使う

## 困ったとき

OSDが表示されないときは、次を確認する。

- ホットキーの`Ctrl+Alt+F12`かタスクトレイのメニューで、OSDを非表示にしていないか
- 前面のアプリが、ゲームとして記録されていないか。[ゲーム中の動作](#ゲーム中の動作)で説明している
- 前面のアプリが、`privacy.blocked_apps`に入っていないか。[プライバシー](#プライバシー)で説明している

起動時に`config.json`を読み込めなかったときは、元のファイルを`config.json.invalid.<数字>.json`に改名して、既定値の`config.json`を作り直す。

原因が分からないときは、ログを確認する。ログは`%LOCALAPPDATA%\ystrokey\logs\ystrokey.log`にある。1ファイルが`diagnostics.max_file_bytes`を超えると、`ystrokey.log.1`のように番号を付けて保存し、`diagnostics.max_files`のファイル数まで残す。既定は、1ファイル1MiBで3ファイル。詳しい情報を出すには、`diagnostics.level`を`debug`にする。設定ウィンドウでは「Level」から変えられる。

## ビルド

必要な環境は、Windows 11とRustのツールチェーン`stable-x86_64-pc-windows-msvc`。

```powershell
cargo build --release
.\target\release\ystrokey.exe
```
