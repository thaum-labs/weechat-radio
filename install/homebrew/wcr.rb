class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.90"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.90/wcr-macos-x86_64.tar.gz"
      sha256 "cf84a2c6b9567ab1dc57d5503da1efce9975e256329f318a9295c538c001b26a"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.90/wcr-macos-aarch64.tar.gz"
      sha256 "4e7796a933acd2b74f8e984746dc0aca1bfa857ecc95f3bda48f661d7ac7653d"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.90/wcr-linux-x86_64.tar.gz"
      sha256 "4201667a76cac11ad73bd3209afd45817dbb8d8148b69616f366d0b8cf3d2423"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.90/wcr-linux-aarch64.tar.gz"
      sha256 "590b919a4d21b77646738db635045d877de595c755523f023e6d3f580a8b3870"
    end
  end

  def install
    bin.install "wcr"
    bin.install "wcr-gui" if File.exist?("wcr-gui")
    bin.install "modem73" if File.exist?("modem73")
    bin.install "radio.py"
  end

  def caveats
    <<~EOS
      Run `wcr setup` once, then `wcr gui` or `wcr tui`.
      Update sha256 lines in this formula after each tagged release (see release CI).
    EOS
  end

  test do
    system "#{bin}/wcr", "--version"
  end
end
