class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.14"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.14/wcr-macos-x86_64.tar.gz"
      sha256 "0f715e0993a819b95aa8d036ea67c13a052a9e2f7bc54f47689c6b69022709f1"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.14/wcr-macos-aarch64.tar.gz"
      sha256 "2aec4e4060cbdf17d06026a20cf328549daf7ab9d2caa476e3e09af0862757a9"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.14/wcr-linux-x86_64.tar.gz"
      sha256 "671fcf2f08f525aeb9ce7ca17f652a32ad55560f8fa20ab476c04d6fff2a1669"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.14/wcr-linux-aarch64.tar.gz"
      sha256 "1ab75f27313ac6dac25e3a3663f0e182ae0e3d7992a4de39b75e74708b8d5aa7"
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
