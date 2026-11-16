class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.18"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.18/wcr-macos-x86_64.tar.gz"
      sha256 "04b0b017a2d041b501d55b73cd087b3993ee4b0c2a8b56c22019b1c7e6abdbeb"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.18/wcr-macos-aarch64.tar.gz"
      sha256 "6ce955b2ecab0ee23b4b4a96d87c7dd96dcc0c48ea7baab5adcf47a4f0a0b255"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.18/wcr-linux-x86_64.tar.gz"
      sha256 "dfaddb1b37ce3712aab7131d980084ce4fc6acfd548287d1b6ff34f44c18b524"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.18/wcr-linux-aarch64.tar.gz"
      sha256 "84eadd5f7dd4a2e7d424e953c8c619b9653ef0da6f4f45afc98ac7e2a4681bce"
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
