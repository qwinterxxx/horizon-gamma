# Gamma

Ultra-lightweight native Gamma mod for [Horizon](https://github.com/horizon-rust/horizon) (Minecraft `26.3`) that toggles brightness between `100%` and `1500%`.

## Features

- Press **G** (rebindable in **Options > Controls**) to toggle between `100%` and `1500%` gamma.
- Toggle setting available in **Options > Mods > Gamma**.
- Displays a brief status toast on toggle.
- Zero allocations on idle frames; updates settings and renderer state only when gamma changes.

## Compatibility

- Pinned to Horizon Nightly `2026-11-20` (`0a875da91dbb0fdcb7ba8c42c11341aa229af51a`) and `horizon-mixins` `e59da9094f0c2fdda6c627fe9eff22dec6a906f0`.

## Build & Install

```sh
cargo horizon-mod check
horizon-launcher native-build . --instance "<instance>" --trust-native-code
```

Or download `gamma-1.0.0.hmod` from [Releases](https://github.com/pantyukovdimathanos-lab/horizon-gamma/releases) and verify its signature with [`horizon-mods.pub`](horizon-mods.pub):

```sh
minisign -Vm gamma-1.0.0.hmod -P RWQjqg8rypRO1FcAIBc+P7vxdFaU1fjCss8evaMaYSBDL21Fy12PGnH8
```

## License

MIT - see [LICENSE](LICENSE).