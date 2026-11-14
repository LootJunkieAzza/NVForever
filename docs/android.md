# Building the experimental Android app

The Android work is tracked by [issue #2436](https://github.com/LootJunkieAzza/NVForever/issues/2436).
It is not a qualified release yet. See [the design and acceptance gaps](design/android.md).

Install Rust stable, Python 3.11+, JDK 17, Gradle 8.11.1, Android SDK platform 35/build tools,
and Android NDK r28c. Set `ANDROID_HOME` and `ANDROID_NDK_HOME` to their installed directories,
and `JAVA_HOME` to JDK 17. On Windows the NDK's `windows-x86_64` toolchain is used.

```sh
rustup target add aarch64-linux-android
python tools/android/build.py --check
python tools/android/build.py
```

`--jobs 2` is the default. `--target-dir PATH` selects an independent Rust build cache;
`--gradle PATH` selects a Gradle executable; `--gradle-build-dir PATH` keeps Gradle output
and its project cache outside the source tree, useful on hosts protecting Documents.
The build uses a separate `android-dev` profile. It leaves release profiles unchanged.
The final command prints the debug APK path. This is a development signing key, not
an official release signature. A release distribution needs its own protected signing
configuration and device qualification.

The helper stages only Git-tracked JSON/YAML/TOML in this repository's schemas and lang
folders. Do not add game files. Install the generated APK using Android's package installer
or `adb install -r <apk>`, then use **Import game folder**. Select a folder copied from your
own lawful game installation. The importer needs enough space for the full Data directory
and leaves source files untouched. No game assets are downloaded or included in the APK.

Developer checks without game data:

```sh
python -m unittest discover -s tools/android/tests
javac -d <temporary-output> android/app/src/main/java/org/nvforever/android/InstallConfig.java android/app/src/test/java/org/nvforever/android/InstallConfigTest.java
java -cp <temporary-output> org.nvforever.android.InstallConfigTest
```

The existing desktop checks in CONTRIBUTING.md still apply to engine changes. A passing
cross-check or Java compilation does not replace desktop regressions or device acceptance.

The `android-test-apk` workflow cross-checks the engine and authored input test source,
builds a development APK, verifies ARM64 ELF/runtime contents and its signature, and
uploads the APK as an artifact. It does not run Android gameplay or input tests on a
device. CI debug signing keys are temporary; an APK from another run may require an
uninstall, which deletes app saves. Keep your own development signing key for updates
and export saves before replacing an installation signed by a different key.
