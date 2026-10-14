class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.85"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.85/wcr-macos-x86_64.tar.gz"
      sha256 "b46eacc180667d4ec946df90e3d56f422f98c22f982e812c86d02674acc53492"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.85/wcr-macos-aarch64.tar.gz"
      sha256 "22951448ee81cc22876cf4eaf9e6c86e7e07034ad83a67dcdf17c3a82c34817c"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.85/wcr-linux-x86_64.tar.gz"
      sha256 "2cd1cafcc8d4495bd8da16d4f0d7392d2bb39df9b09831c1ec4415190863b9ef"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.85/wcr-linux-aarch64.tar.gz"
      sha256 "6568ec8f63239235100de83cfeba2a7e77119a9361badf185237575015340294"
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
