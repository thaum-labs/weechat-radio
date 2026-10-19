class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.98"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.98/wcr-macos-x86_64.tar.gz"
      sha256 "4166b786ae676d9a6fec2b64de66269491af23ad6e22ce2518e329e68f72c1f7"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.98/wcr-macos-aarch64.tar.gz"
      sha256 "f5272cddd92fb66fcf1b70e70c44903077f3be689b1be60f68f1e685593d4230"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.98/wcr-linux-x86_64.tar.gz"
      sha256 "2de47b1c5dfb40a843c4e59868944ca77c0c88abdfb560bf2b4272049973ba22"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.98/wcr-linux-aarch64.tar.gz"
      sha256 "c862bab026d799100fc4c8998c9e11ab777b2e4a0a1705abdd8cc17d34185346"
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
