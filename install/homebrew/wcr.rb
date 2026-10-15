class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.89"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.89/wcr-macos-x86_64.tar.gz"
      sha256 "8fe0123af7ede02aff6fdde73c8ab1d9f5f72319cf814b2ddca5574301ddbd31"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.89/wcr-macos-aarch64.tar.gz"
      sha256 "81cab3293d6f32efaaafbd682814b834effbe0e0b352612a661284fe97224929"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.89/wcr-linux-x86_64.tar.gz"
      sha256 "f9efb76f3c5353c49112ee11de81a61bb229724c609a84b31b7af59c1a0d2daf"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.89/wcr-linux-aarch64.tar.gz"
      sha256 "464963cf79e4df8870e39ee8d701add50e48a0559648b5bb1ea6bb5d9e6c2236"
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
