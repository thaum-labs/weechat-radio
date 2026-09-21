class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.14"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-x86_64.tar.gz"
      sha256 "eccfdcd885ff2c54c86126f4add36fc4d9ba6539dc9461528dcba77e2de214ae"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-aarch64.tar.gz"
      sha256 "a173688b59fd11718ddfe24b4af53de19c1cbffec3c9c9cbfc91977b14b38854"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-x86_64.tar.gz"
      sha256 "363774eaccad7feb114d6d28561bf3665f071bc07d487df3ac53daed64dda3ab"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-aarch64.tar.gz"
      sha256 "900fc9a0995f89cb1b909b88e86899c060e8ee747c48efe6db0c2b7f6c5e4e9d"
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
