class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.104"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.104/wcr-macos-x86_64.tar.gz"
      sha256 "4385b17eea9ab22f389ee8c729591484eb6ccb98899b079adf757c24a34960d9"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.104/wcr-macos-aarch64.tar.gz"
      sha256 "5103a849e45e41899085ee8c46ffc304f1203c667f6b08e92bdcafa4d55d7faa"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.104/wcr-linux-x86_64.tar.gz"
      sha256 "ec8611ea970fb3d90409e98c2794368caac7a467b4b87db01d39349f3f0c718c"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.104/wcr-linux-aarch64.tar.gz"
      sha256 "d0ffc7d01f6d2960ba78d3e499d304c343bf26cc66fa7982d23c9b3fb05fb49d"
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
