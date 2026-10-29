class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.3"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.3/wcr-macos-x86_64.tar.gz"
      sha256 "98e283b2cbe4fc9838fd6ed4e37506c4f6adec8077c6e017035f2fa06a411295"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.3/wcr-macos-aarch64.tar.gz"
      sha256 "56d1e312f4ab44eed332561f9f543588d2cd02bef1833db650b13209f47dab47"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.3/wcr-linux-x86_64.tar.gz"
      sha256 "8cf91a1e891f78c5e5e97bcd61e9fdd51df7bd5653a0ee27cfe862ab4a2205c4"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.3/wcr-linux-aarch64.tar.gz"
      sha256 "103bbdf66ec610ffb0c2b01631233e5ac51ec9501e2b8f68c13e069ccf70ffdd"
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
