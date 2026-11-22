# Gamma

Ultra-lightweight native Gamma boost mod for [Horizon](https://github.com/horizon-rust/horizon) (Minecraft `26.3`) that toggles render brightness boost (`1500%`) without altering your saved `options.txt` gamma.

## Features

- Press **G** (rebindable in **Options > Controls**) to toggle the `1500%` gamma boost on or off.
- Toggle setting available in **Options > Mods > Gamma**.
- Applies the boost strictly to `renderer.gamma`, leaving `settings.gamma` (`options.txt`) untouched: non-default brightness settings survive options saves while boosted, turning the boost off restores your configured brightness, and removing the mod leaves zero stale `1500%` values behind.
- Respects Horizon's external-server fullbright safeguard (`fullbright_held`): leaves normal brightness intact on external servers and LAN guest sessions, while remaining active in singleplayer, hosted LAN worlds, and Replay Studio.
- Zero allocations on idle frames.

## Compatibility

- Pinned to Horizon Nightly `2026-11-20` (`0a875da91dbb0fdcb7ba8c42c11341aa229af51a`) and `horizon-mixins` `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`.

## Build & Install

```sh
cargo horizon-mod check
horizon-launcher native-build . --instance "<instance>" --trust-native-code
```

Or download `gamma-1.0.2.hmod` from [Releases](https://github.com/qwinterxxx/horizon-gamma/releases) and verify its signature with [`horizon-mods.pub`](horizon-mods.pub):

```sh
minisign -Vm gamma-1.0.2.hmod -P RWQjqg8rypRO1FcAIBc+P7vxdFaU1fjCss8evaMaYSBDL21Fy12PGnH8
```

## License

MIT - see [LICENSE](LICENSE).
