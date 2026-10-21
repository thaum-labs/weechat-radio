class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.105"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.105/wcr-macos-x86_64.tar.gz"
      sha256 "db126c062d322036f4c859de111d395e16c280b31407ceacc65c4fba95c5c646"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.105/wcr-macos-aarch64.tar.gz"
      sha256 "e6b8987b9cd4d57e3cfc7602d0f6751635b058896b34487468db1795c3f1e3ca"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.105/wcr-linux-x86_64.tar.gz"
      sha256 "6d8d7983810689e9c4c6c69d89ba958be1a533b9f091ba14c8d1c991c6deb1c6"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.105/wcr-linux-aarch64.tar.gz"
      sha256 "b5e5e9b0de7e01e53c5c2404e3f943d5107e557608d1203934c6ce66bcab1459"
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
