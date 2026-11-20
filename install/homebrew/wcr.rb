class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.20"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.20/wcr-macos-x86_64.tar.gz"
      sha256 "99db47659480e4b028df54d65ebe0da6ca64a64ae573cd0d896cde4850b82241"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.20/wcr-macos-aarch64.tar.gz"
      sha256 "c211d7ab3d2ccac80c19499b15befe1f1eba1fed2e8e6fa7305841b35756e6b2"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.20/wcr-linux-x86_64.tar.gz"
      sha256 "5bb4f801957b4faec40c23052c26ff1861e46c20106f9906c418bb3ab6e5ec35"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.20/wcr-linux-aarch64.tar.gz"
      sha256 "ac3009df9e5477c0545f9c3c6eabb420fba182ddebb4d39d96a05b01be9cd48e"
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
