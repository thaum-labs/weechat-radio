class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.12"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.12/wcr-macos-x86_64.tar.gz"
      sha256 "89d125fb5105aee4cd4d601aa2660e825a1bd51f757734a8c8a72656bd79cc87"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.12/wcr-macos-aarch64.tar.gz"
      sha256 "431bac919083efa161acb7fd0b86f33b79afa2cd5ffee41da9dc3071065349be"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.12/wcr-linux-x86_64.tar.gz"
      sha256 "773423629cc95cace33d1172a1010e666005b0021acdb876c22fba0a4f0e33ed"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.12/wcr-linux-aarch64.tar.gz"
      sha256 "7a729a8d6e731758e709c3f6de1b635d55264ec59a8fb2bdc2ff627002612bc3"
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
