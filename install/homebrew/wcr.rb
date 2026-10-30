class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.5"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.5/wcr-macos-x86_64.tar.gz"
      sha256 "39499e36e773db34702a66b66a0d9fb697c94b3244812f6878d5a031a973d6b3"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.5/wcr-macos-aarch64.tar.gz"
      sha256 "d69a616e0d5d74c734dae2533922ccbcf590cc74f135354b997f0e6463e5fbfa"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.5/wcr-linux-x86_64.tar.gz"
      sha256 "9d4af7d8937188e3341320720a225fae58f26315cd23afe3f839e30b88e411c4"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.5/wcr-linux-aarch64.tar.gz"
      sha256 "67b139f00457e22ae9c308cfc883cdcfaec24c41ced113ab1709e08e1927360d"
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
