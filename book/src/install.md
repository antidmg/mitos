# Installing Mitos

Build Mitos from source using the instructions in the
[building guide](./building-from-source.md).

Note that:

- To take full advantage of Mitos, install the language servers for your
  preferred programming languages. See the
  [wiki](https://github.com/helix-editor/helix/wiki/Language-Server-Configurations)
  for instructions.

## Pre-built binaries

Download pre-built binaries from the [GitHub Releases page](https://github.com/matoous/mitos/releases).
The tarball contents include an `mitos` binary and a `runtime` directory.
To set up Mitos:

1. Add the `mitos` binary to your system's `$PATH` to allow it to be used from the command line.
2. Copy the `runtime` directory to a location that `mitos` searches for runtime files. A typical location on Linux/macOS is `~/.config/mitos/runtime`.

To see the runtime directories that `mitos` searches, run `mitos --health`. If necessary, you can override the default runtime location by setting the `MITOS_RUNTIME` environment variable.
