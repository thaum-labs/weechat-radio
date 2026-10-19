class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.99"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.99/wcr-macos-x86_64.tar.gz"
      sha256 "f097291203730988c523ed20ed7a7e7175f7921690fbaa51dbf2f4b86d6c40e4"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.99/wcr-macos-aarch64.tar.gz"
      sha256 "989653c1fe3d5a618e8b764e54149dfd9d576a2cd0dabadee6dde6aa925ac95e"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.99/wcr-linux-x86_64.tar.gz"
      sha256 "94857fcc3f1c2f04f877d4bbb8a607ab049031b8bbbc47fa9a699254d4b55497"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.99/wcr-linux-aarch64.tar.gz"
      sha256 "f3618ee5c10aadc63d0a1ce3b79025fdcf427be60254dce4c585e31e3fde567f"
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
