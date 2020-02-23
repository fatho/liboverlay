# liboverlay

LD_PRELOAD hack for emulating a rudimentary overlayfs-like file system structure without actually using overlayfs.
The goal of this project was to make a proof of concept, it is not intended for production use.
The primary use case was to enable running a program from a read-only directory
even though it assumes that its directory is writable.

Limitations:

- will not work when using the `<...>at` variants of certain libc functions with relative paths
- cannot fake deletion of a file from a lower directory
- only works for programs dynamically linking libc, it does not intercept the system calls directly
- only supports one lower dir, not multiple like overlayfs
- does not intercept mode changes
- probably some more

If you're daring enough to try this out yourself, you can compile the library with `cargo`:

```bash
cargo build
```

This will produce the so file in `target/debug/liboverlay.so`.

You can then run a program of your choice with an overlayfs emulation.
The upper and lower directories can be configured with the environment variables `LIBOVERLAY_UPPER_DIR` and `LIBOVERLAY_LOWER_DIR`.

```
LD_PRELOAD=/absolute/path/to/liboverlay.so \
LIBOVERLAY_UPPER_DIR=/absolute/path/to/writable/upper/dir \
LIBOVERLAY_LOWER_DIR=/absolute/path/to/readonly/lower/dir \
./some_executable
```