class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.25"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.25/wcr-macos-x86_64.tar.gz"
      sha256 "555c3a3736408e5136b8d9788c51cfe2eac07b5499c25ad11274f49be6b483cf"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.25/wcr-macos-aarch64.tar.gz"
      sha256 "c1bc70a9acf9467aa2a78f3f387d4ce831e0c7e5d639f8727194dd9cad76b495"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.25/wcr-linux-x86_64.tar.gz"
      sha256 "bb0953ad6f2787ecbdaee7a3df3d43258769862c5b9c2cbfcdfef5ce6e829fdb"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.25/wcr-linux-aarch64.tar.gz"
      sha256 "dd4d9f8bd8f2dd6bc5df54d63376e50a97ed7df2d19593473ab4b1e628c75608"
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
