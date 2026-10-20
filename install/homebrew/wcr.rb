class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.102"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.102/wcr-macos-x86_64.tar.gz"
      sha256 "0c5383a1e955927480cf169060929a4eb1bb8de3a55369bc9273971c8ccbb9dd"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.102/wcr-macos-aarch64.tar.gz"
      sha256 "51508569aa09bd4c16e532901a8eeeabaec26fa08192973869a064f1430bcbb2"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.102/wcr-linux-x86_64.tar.gz"
      sha256 "55f2de884e833bde8a59f253613df01ed1de413f8876f55adafd3c7ae92b85fb"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.102/wcr-linux-aarch64.tar.gz"
      sha256 "c4ef521df7d1f148423784b66765c00cf820f05dc9f4ea51b60d6b0004367b49"
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
