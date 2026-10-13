class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.82"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.82/wcr-macos-x86_64.tar.gz"
      sha256 "bcff5f2afb68efad0456b4cb6354cd2a3b04fa4be013d60f2a9a9e5c3e05425e"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.82/wcr-macos-aarch64.tar.gz"
      sha256 "ecdd8482cdd41d053f5be12b8f261c795a8a274c552278dc3bcfa25083a9c6d5"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.82/wcr-linux-x86_64.tar.gz"
      sha256 "12a417267c4a997c3c595c373ec63ad458c6d5c60eb6fccdad83db46b3f282d1"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.82/wcr-linux-aarch64.tar.gz"
      sha256 "c596536d9ad93d35d8bac2787ea610cb386d00ca6644c2b98ab7c70607b68719"
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
