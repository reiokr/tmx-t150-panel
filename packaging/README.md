# Prebuilt package

`install.sh` is the installer for the tarball attached to a release. It copies
the binary to `~/.local/bin` (or `/usr/local/bin` with `--system`), writes the
menu entry and installs the icon. `uninstall.sh` removes them again; profiles in
`~/.config/tmx-t150-panel` are kept.

```sh
tar xzf tmx-t150-panel-*.tar.gz
cd tmx-t150-panel-*/
./install.sh
```

The package contains no kernel module binary. Force feedback and the wheel
settings (rotation, gain, auto-centre) come from `hid-t150`, which has to be
built for your running kernel:

```sh
cd driver/t150
sudo ./install.sh
```

## Building the tarball

```sh
packaging/make-release.sh              # Debian bookworm container (glibc 2.36)
NO_DOCKER=1 packaging/make-release.sh  # local toolchain, newer glibc required
```

The output lands in `target/package/` with a `.sha256` next to it. The container
build is the default on purpose: a binary built on a rolling distribution needs
a recent glibc and will not start on an LTS release.
