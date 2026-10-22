# yStrokey

入力したキーを画面に表示する、Windowsのオンスクリーンディスプレイ(OSD)ツール。用途は入力したキーが意図どおりかの確認や、配信・録画・解説でのキーの表示。

## 特徴

- 入力したキーを、画面の最前面の小さなOSDに表示する。OSDはクリックを透過するため、下のウィンドウをそのまま操作できる
- テンキーは`Num1`のように表示し、上段の数字キーと区別する。`Ctrl+C`のようなショートカットには、`Copy`などのラベルを表示する
- IMEで入力中の未確定の文字列も表示する
- 同じキーを続けて押すと、押した回数を表示する。短い間隔で押した別々のキーは、1行にまとめて表示する
- 前面のウィンドウがあるモニターにOSDを移動する。位置はモニターごとに保存する
- 既定では`Ctrl+Alt+F12`で、OSDの表示と非表示を切り替える。キーは`hotkey.toggle`で変更できる

## インストール

[Scoop](https://scoop.sh/)でインストールする。

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install yscoopy/ystrokey
```

`ystrokey`を起動すると、タスクトレイにアイコンが表示される。アイコンを右クリックし、メニューの「設定」から設定ウィンドウを開く。設定は`%APPDATA%\ystrokey\config.json`に保存される。設定は設定ウィンドウのほか、`config.json`を直接編集しても変更できる。

## 注意

- ゲームが前面にある間は、OSDの表示とキー入力の取得を停止する。前面のウィンドウがゲームかどうかは、WindowsがXbox Game Barのために記録しているゲームの一覧で判定する。ゲームが前面にある間も停止しないようにするには、`behavior.pause_in_games`を`false`にする
- `privacy.blocked_apps`に指定したアプリが前面にある間は、OSDに何も表示しない。`privacy.blocked_apps`の既定値は、パスワードマネージャーの`KeePass.exe`と`1Password.exe`
- クリップボードにコピーした文字列は、既定では表示しない。表示するには、`behavior.show_clipboard`を`true`にする
- 動作確認はWindows 11のみ

## アンインストール

1. タスクトレイのメニューの「自動起動」にチェックが付いていれば、選んでチェックを外す
2. タスクトレイのメニューの「終了」を選んで、yStrokeyを終了する
3. `scoop uninstall ystrokey`を実行する

アンインストール後も、設定は`%APPDATA%\ystrokey\`に、ログは`%LOCALAPPDATA%\ystrokey\`に残る。設定とログが不要なら、2つのフォルダを削除する。

## ビルド

Rustのツールチェーン`stable-x86_64-pc-windows-msvc`でビルドする。

```powershell
cargo build --release
.\target\release\ystrokey.exe
```

## ライセンス

[MIT License](LICENSE)
