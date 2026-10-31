class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.7"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.7/wcr-macos-x86_64.tar.gz"
      sha256 "7a4434aee7dda60250a1dfd9aa1253c4600d1c2b3b361ad879914cd4eb819e55"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.7/wcr-macos-aarch64.tar.gz"
      sha256 "930c080693762a07bc0a4697b16814851481d9c9ff1ea06b211a2096dba48b66"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.7/wcr-linux-x86_64.tar.gz"
      sha256 "9d215e71040925773c18ce8629529977faa2cef0f467549cd3a4cd0df9377b00"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.7/wcr-linux-aarch64.tar.gz"
      sha256 "79eebfbd42c0e2c8910280f2be80c23dda44552a326a6e96d6a2b114004eca03"
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
