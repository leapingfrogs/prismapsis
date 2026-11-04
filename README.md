# Prismapsis

A gravity-bouncing, colour-collecting shooter. Your hero bounces through
drained, greyscale levels with a glass prism in orbit, splitting a beam of
light to bring colour back and open the way to the exit.

**[Play it in your browser](https://leapingfrogs.github.io/prismapsis/)**, on
a computer, phone or tablet. On a phone, add the page to your home screen to
play without the browser's bars.

Downloads for Linux, Windows and macOS are on the
[releases page](https://github.com/leapingfrogs/prismapsis/releases).

## Controls

| Action | Keyboard | Gamepad |
|---|---|---|
| Spin left or right | A / D or arrow keys | D-pad left and right, or the shoulder buttons |
| Fire the beam (hold) | Space | South, or right trigger |
| Freeze the prism (hold) | Shift | Left trigger |
| Nudge a frozen prism | Q / E | West / East |
| Move up and down, with anti-gravity | W / S or arrow keys | D-pad up and down |
| Light nova, once collected | N | North |
| Pause | P | Start |
| Fullscreen or a window | F11 | |

Phones and tablets get on-screen buttons.

## Running the downloads

The builds aren't code-signed, so your system will warn you the first time:

- **Windows:** SmartScreen may say it protected your PC. Choose "More info",
  then "Run anyway".
- **macOS:** right-click the program, choose "Open", and confirm. If macOS
  still refuses, allow it under System Settings, Privacy & Security.
- **Linux:** unpack the archive and run `./prismapsis`.

## Checking a download

From 0.1.1, each release has a `SHA256SUMS` file, to check a download is
intact with `sha256sum --check --ignore-missing SHA256SUMS`. GitHub's CI also
signs a record of where each build came from, which the
[GitHub CLI](https://cli.github.com/) can check:

```
gh attestation verify <file> --repo leapingfrogs/prismapsis
```

This shows a file was built from this repository. It doesn't stop the warnings
above, which need paid code signing.

## Building from source

The project uses [devenv](https://devenv.sh/) for its tools. Run `devenv
shell`, then `just run` to play a dev build, or `just` to list every task. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the workflow and
[docs/design.md](docs/design.md) for the design.

## Licence

The code is available under either the [MIT](LICENSE-MIT) or the
[Apache 2.0](LICENSE-APACHE) licence, at your option. The Exo 2 font is under
the [SIL Open Font License](assets/fonts/Exo2-OFL.txt).
