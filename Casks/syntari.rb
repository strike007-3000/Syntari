cask "syntari" do
  version :latest
  sha256 :no_check # asset is replaced on every build

  url "https://github.com/strike007-3000/Syntari/releases/download/macos-latest/Syntari-macos.dmg"
  name "Syntari"
  desc "Private on-device meeting synthesizer"
  homepage "https://github.com/strike007-3000/Syntari"

  depends_on arch: :arm64
  depends_on macos: :ventura

  app "Syntari.app"

  # Ad-hoc signed, not notarized: drop the quarantine flag so Gatekeeper doesn't block launch.
  # Legacy `postflight` is used on purpose: it also works on older Homebrew versions.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Syntari.app"]
  end
end
