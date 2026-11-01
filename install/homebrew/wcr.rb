class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.9"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.9/wcr-macos-x86_64.tar.gz"
      sha256 "fe4ee700721f62db106299de680c345b143bc445caafa449377b41fc14db6d66"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.9/wcr-macos-aarch64.tar.gz"
      sha256 "ba6398657472ca1a5542471828d9f5597eaa71db926638b2ad0d3204dee10a3a"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.9/wcr-linux-x86_64.tar.gz"
      sha256 "6149920d9aafaadbe4e0acc674feb137555f13decc1850965bc2d3ce379eedd3"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.9/wcr-linux-aarch64.tar.gz"
      sha256 "dda3f60580963908141e177cbc51f0beff0fb14bd3df367e1d2db93e4543f938"
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
