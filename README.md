<img src="./artwork/banner.png" style="border-radius: 6px; margin-bottom: 8px">

## 📜 License

This branch is **view-only**. See the [LICENSE](./LICENSE) file for details.

## 📦 Repository

Accompanist released a group of artifacts, including: 

- [`lyrics-core`](https://github.com/6xingyv/Accompanist-Lyrics) - Parsing lyrics file, holding data and exporting to other formats.

- [`lyrics-ui`](https://github.com/6xingyv/Accompanist) - Standard lyrics interface built on Jetpack Compose

This repository hosts the `lyrics-ui` code.

## ✨ Features

- **🎤 Multi-Voice & Duet Support**: Effortlessly display lyrics for multiple singers.

- **🎶 Accompaniment Line Support**: Styles main vocals from accompaniment lines.

- **⚡️ High-Performance Rendering**: Engineered for buttery-smooth animations and low overhead, ensuring a great user experience even on complex lyrics.

## 🐧 Linux AppImage

On an x86_64 or aarch64 Linux host with the Rust and native build dependencies
installed, build the portable desktop package with:

```bash
bash packaging/appimage/build-appimage.sh
```

The result is written to `dist/Accompanist-Lyrics-<version>-<arch>.AppImage`.
On Linux, the desktop renderer uses MPRIS2 over the session D-Bus for active
player metadata, playback position, lyric-click seeking, and album artwork.
On first launch it creates `config.json` and an empty `lyrics/` folder beside the
AppImage when that directory is writable, otherwise under
`$XDG_CONFIG_HOME/accompanist-lyrics` (or `~/.config/accompanist-lyrics` when
`XDG_CONFIG_HOME` is unset). Set `LINUXDEPLOY` to use an existing linuxdeploy
binary instead of downloading one.

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a pull request or open an issue to discuss your ideas. For major changes, please open an issue first.
