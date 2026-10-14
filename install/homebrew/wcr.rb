class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.86"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.86/wcr-macos-x86_64.tar.gz"
      sha256 "5c94099736ac5411d28a61a050c5bef53ec185e86da55e89e305848b7866ecb2"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.86/wcr-macos-aarch64.tar.gz"
      sha256 "4705ba28238848c4c2e532ea264b3c7f76538b73fd6cbea8a2f23e02b66ef012"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.86/wcr-linux-x86_64.tar.gz"
      sha256 "1c86d6c47121e1cfa271a128e118c8ab0a2f315b8286003991a66f90d0c4689a"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.86/wcr-linux-aarch64.tar.gz"
      sha256 "237e9fb60831e8060c91304c7fa614cb7323d7f8a7f42af3cd9a45cb6243bc48"
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
