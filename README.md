# zed-tsrs

This Zed extension downloads and uses [`tsrs`](https://github.com/oxc-project/tsrs) language server from npm.
It resolves the latest package version when checking for installation or updates.

The extension supports the following platforms and uses the corresponding npm package:

| Platform                    | npm package                                                                    |
| --------------------------- | ------------------------------------------------------------------------------ |
| macOS ARM64 (Apple silicon) | [`@tsrs/darwin-arm64`](https://www.npmjs.com/package/@tsrs/darwin-arm64)       |
| Linux ARM64 with glibc      | [`@tsrs/linux-arm64-gnu`](https://www.npmjs.com/package/@tsrs/linux-arm64-gnu) |
| Linux x64 with glibc        | [`@tsrs/linux-x64-gnu`](https://www.npmjs.com/package/@tsrs/linux-x64-gnu)     |

Intel macOS, Windows, and MUSL Linux distributions are not supported by this extension.
