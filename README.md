# yStrokey

押したキーを画面に表示する、Windowsのオンスクリーンディスプレイ(OSD)ツール。押したキーが意図どおりかを確かめたい人や、配信・録画・解説で押したキーを見せたい人に向けている。

## 特徴

- 押したキーを、画面の最前面の小さなOSDに表示する。OSDはクリックを通すので、下のウィンドウをそのまま操作できる
- テンキーを通常の数字キーと区別し、`Ctrl+C`のようなショートカットには`Copy`などのラベルを付ける
- IMEで入力中の未確定の文字列も表示する
- 同じキーの連打回数を数え、短い間隔の連続入力を1つにまとめる
- 前面のウィンドウがあるモニターにOSDを移す。位置はモニターごとに保存する
- ホットキーの`Ctrl+Alt+F12`で、OSDの表示と非表示を切り替える

## インストール

[Scoop](https://scoop.sh/)で入れる。

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install yscoopy/ystrokey
```

`ystrokey`を起動すると、タスクトレイにアイコンが出る。アイコンを右クリックし、メニューの「設定」から設定ウィンドウを開く。設定は`%APPDATA%\ystrokey\config.json`に保存され、`config.json`を直接編集してもよい。

## 注意

- ゲームが前面にある間は、OSDの表示とキー入力の取得を止める。ゲームの判定には、WindowsがXbox Game Barのために記録しているゲームの一覧を使う。ゲーム中も表示するには、`behavior.pause_in_games`を`false`にする
- `privacy.blocked_apps`のアプリが前面にある間は、OSDに何も表示しない。既定はパスワードマネージャーの`KeePass.exe`と`1Password.exe`
- クリップボードにコピーした文字列は、既定では表示しない。表示するには、`behavior.show_clipboard`を`true`にする
- 動作を確認したのはWindows 11だけ

## アンインストール

1. トレイのメニューの「自動起動」にチェックが付いていれば、選んで外す
2. トレイのメニューの「終了」で終了する
3. `scoop uninstall ystrokey`を実行する

設定は`%APPDATA%\ystrokey\`に、ログは`%LOCALAPPDATA%\ystrokey\`に残る。不要なら削除する。

## ビルド

Rustのツールチェーン`stable-x86_64-pc-windows-msvc`でビルドする。

```powershell
cargo build --release
.\target\release\ystrokey.exe
```

## ライセンス

[MIT License](LICENSE)
