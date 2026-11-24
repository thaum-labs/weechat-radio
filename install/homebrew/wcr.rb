class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.24"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.24/wcr-macos-x86_64.tar.gz"
      sha256 "50f73448e72c3a012ea1d9dc09f920e25b5beeabe8eb03637508d25f300ab4a1"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.24/wcr-macos-aarch64.tar.gz"
      sha256 "53ffbdc49370bb91034fa92c01dc03609cf83505bdd07eaeaac0badae83ae0e8"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.24/wcr-linux-x86_64.tar.gz"
      sha256 "c70b453a7c072b462f1a25afeff8b49037c23d335376f0d41498dc9f80e54657"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.24/wcr-linux-aarch64.tar.gz"
      sha256 "02035568bf314a773675ab41bfd7ef0b87d39472d38f41689558176448c98870"
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
