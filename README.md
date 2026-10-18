# yStrokey

入力したキーを画面に表示する、Windowsのオンスクリーンディスプレイ(OSD)ツール。用途は、入力したキーが意図どおりかの確認や、配信・録画・解説でのキーの表示。

## 特徴

- 入力したキーを、画面の最前面の小さなOSDに表示する。OSDはクリックを透過するため、下のウィンドウをそのまま操作できる
- テンキーを通常の数字キーと区別し、`Ctrl+C`のようなショートカットには`Copy`などのラベルを表示する
- IMEで入力中の未確定の文字列も表示する
- 同じキーの連打回数を表示し、短い間隔の連続入力を1つにまとめて表示する
- 前面のウィンドウがあるモニターにOSDを移動する。位置はモニターごとに保存する
- ホットキーの`Ctrl+Alt+F12`で、OSDの表示と非表示を切り替える

## インストール

[Scoop](https://scoop.sh/)でインストールする。

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install yscoopy/ystrokey
```

`ystrokey`を起動すると、タスクトレイにアイコンが表示される。アイコンを右クリックし、メニューの「設定」から設定ウィンドウを開く。設定は`%APPDATA%\ystrokey\config.json`に保存される。設定ウィンドウのほか、`config.json`の直接編集でも設定を変更できる。

## 注意

- ゲームが前面にある間は、OSDの表示とキー入力の取得を停止する。ゲームの判定には、WindowsがXbox Game Barのために記録しているゲームの一覧を使用する。ゲーム中も表示するには、`behavior.pause_in_games`を`false`にする
- `privacy.blocked_apps`のアプリが前面にある間は、OSDに何も表示しない。既定はパスワードマネージャーの`KeePass.exe`と`1Password.exe`
- クリップボードにコピーした文字列は、既定では表示しない。表示するには、`behavior.show_clipboard`を`true`にする
- 動作確認はWindows 11のみ

## アンインストール

1. タスクトレイのメニューの「自動起動」がチェックされていれば、選択して解除する
2. タスクトレイのメニューの「終了」で終了する
3. `scoop uninstall ystrokey`を実行する

アンインストール後も、設定は`%APPDATA%\ystrokey\`に、ログは`%LOCALAPPDATA%\ystrokey\`に残る。不要なら削除する。

## ビルド

Rustのツールチェーン`stable-x86_64-pc-windows-msvc`でビルドする。

```powershell
cargo build --release
.\target\release\ystrokey.exe
```

## ライセンス

[MIT License](LICENSE)
