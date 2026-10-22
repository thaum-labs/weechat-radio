class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.114"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.114/wcr-macos-x86_64.tar.gz"
      sha256 "d10671b2147aeca63b407b6284af9fcf7b96ba9d464447747451cb2fe7008a11"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.114/wcr-macos-aarch64.tar.gz"
      sha256 "fd169920b8b8a927bb2f9131410ea85415294b883224869f8bfccd3133908045"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.114/wcr-linux-x86_64.tar.gz"
      sha256 "1754ceae29bd27e5a1b7f7e07a7e34009b59d01db2040653537a928528d890f7"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.114/wcr-linux-aarch64.tar.gz"
      sha256 "a6e36d4f74649afc984078c78b19b3599fcd5ab49fc3ff898e0697fa1590fa3a"
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
