class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.11"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.11/wcr-macos-x86_64.tar.gz"
      sha256 "68363cc60ce9feb07b48134c4dd3cebe98173c2d418e1daee99523b5e3c2c4ad"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.11/wcr-macos-aarch64.tar.gz"
      sha256 "23abef17164104395922fb66c4a26004d061da5f7adce28ca9a92f171ddde85e"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.11/wcr-linux-x86_64.tar.gz"
      sha256 "ce13a440cb09953513f747935ad01caf0efa3da7cbef79506b8bbee19d5f9296"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.11/wcr-linux-aarch64.tar.gz"
      sha256 "826221809fd4aa755cabc1b33bea054823eeb0f89920139ae80e6dae7d0bcdfb"
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
