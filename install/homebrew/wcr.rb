class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.113"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.113/wcr-macos-x86_64.tar.gz"
      sha256 "1faa5a1bcb771562eaa1cf9c1a18930fc25fe7517b8d045c957544f4a3e98773"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.113/wcr-macos-aarch64.tar.gz"
      sha256 "63a58dc088c744033d8e04d968eec10aef6703b59dfe11391ff65150ce42233b"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.113/wcr-linux-x86_64.tar.gz"
      sha256 "988cdde0d88e40131d513c75625b96924c945a309ca2c1a29912465f798a4c6c"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.113/wcr-linux-aarch64.tar.gz"
      sha256 "146c6af0a8398ee9ab89f0244ebee0275ee56d9042e0050149bec2136c02ee2e"
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
