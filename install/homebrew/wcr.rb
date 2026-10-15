class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.88"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.88/wcr-macos-x86_64.tar.gz"
      sha256 "cbdc1574a16430895f1edae6209e08baadbde12ef71afc687fc9baa4a0cf5ab9"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.88/wcr-macos-aarch64.tar.gz"
      sha256 "1318770b70a94306a424d2cdea07313294a5c517723330bef07c3a881d522724"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.88/wcr-linux-x86_64.tar.gz"
      sha256 "2d9ad89b49c96db45909d794d8c5aaa5431fd097fc1ab63f7f259557ad306f5d"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.88/wcr-linux-aarch64.tar.gz"
      sha256 "d9d9d44c1949a58704b4afde0f2696e502fe7f0a0f473ab9d99d1132f6d474b1"
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
