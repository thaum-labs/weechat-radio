class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.8"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.8/wcr-macos-x86_64.tar.gz"
      sha256 "eac11328be57928c23dc35c8165b8c18c4db80892b1548c7a69ce6d52facc87e"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.8/wcr-macos-aarch64.tar.gz"
      sha256 "62445d24a3fbb854d026fe030078c48f3c85b201c278b19b1e8517c9b4136502"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.8/wcr-linux-x86_64.tar.gz"
      sha256 "92519f1cab2677be10cb9f5bd4b3fbe2c900d21d1d5d579c79b8e73ed4a2cd9d"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.8/wcr-linux-aarch64.tar.gz"
      sha256 "e7106cdd8ddd69bfb405718a8c8bf1fff8d5af79f966ef655fbead28ca9ea870"
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
