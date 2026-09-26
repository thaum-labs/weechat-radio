class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.78"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.78/wcr-macos-x86_64.tar.gz"
      sha256 "04d9a59530996a41b118baa36e33a82e0fa0d8dd424aed4e2befccc8d2e2b331"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.78/wcr-macos-aarch64.tar.gz"
      sha256 "084d1578af23453bf4591cd3b3715fd96c8a5271239a0ce2fe5aa47775761491"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.78/wcr-linux-x86_64.tar.gz"
      sha256 "1638a8fe48445737908911cffa6320a2963fc03ae1a0fe2c00cc7168c3279115"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.78/wcr-linux-aarch64.tar.gz"
      sha256 "6da5d04b96e1987ab5996d3b32cf4391ac0d7d4ef0392779a1804df3596be5de"
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
