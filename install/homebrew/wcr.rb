class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.1"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.1/wcr-macos-x86_64.tar.gz"
      sha256 "3c78e9d345bc4bb62e7e162c2d650323b21a817bb45f770f491802539017b723"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.1/wcr-macos-aarch64.tar.gz"
      sha256 "fe98cd89b5852bfe33204245e687d29fb69c57b63ea8e2862176b244c084122b"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.1/wcr-linux-x86_64.tar.gz"
      sha256 "ed49b3f5447d591038c1c3f365c957913594b1d0453b70c3333c5e1db8b75051"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.1/wcr-linux-aarch64.tar.gz"
      sha256 "b69c53e922b445f56008eec20ae48322f9e0d377c9e388d0b6459b9d1c7fc76b"
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
