class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.87"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.87/wcr-macos-x86_64.tar.gz"
      sha256 "fb7c6acd09c4769432bdf3c22aebf00fede9a2bc858a1fb2142d907b76823c1f"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.87/wcr-macos-aarch64.tar.gz"
      sha256 "0c8acdc52f17cc255bcf3c1643fe20c8c474c19f91222d4060275011f38ca477"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.87/wcr-linux-x86_64.tar.gz"
      sha256 "e4bde1fa8e94b4f35aece084adccb8bdf5695981cd36d59a14e78c12bce148a1"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.87/wcr-linux-aarch64.tar.gz"
      sha256 "5ac986a0b6650ab98886daac8685f9ff1d3c520e85ea4b882b2451ffb39aa6d4"
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
