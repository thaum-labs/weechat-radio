class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.16"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.16/wcr-macos-x86_64.tar.gz"
      sha256 "da5c17b87ec73f73657a6ce4736d3df2548304388b7fd242ed544dc353b89a35"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.16/wcr-macos-aarch64.tar.gz"
      sha256 "810d54c016a9b1c201419fce86a2cf3cf429959a58574734aacc2d82bfe212be"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.16/wcr-linux-x86_64.tar.gz"
      sha256 "aa2689e16427f3bcdb59fa3b283b9aceedd8e3891d5b6c7559f717d4a5715ca9"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.16/wcr-linux-aarch64.tar.gz"
      sha256 "f7490fc0f1220353c83b9b985bee5ec21d7d25480442bc19ea585a8db6096486"
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
