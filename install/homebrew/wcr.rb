class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.14"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-x86_64.tar.gz"
      sha256 "436cb19c5545d86a162d46e1ba83a8b1ed85ff9f5649442d3a00a9c3b75d5e44"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-aarch64.tar.gz"
      sha256 "b76cf02a3b8caf7ff05af38a1bc7a19ad3577e4824980e1764977920b8091f39"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-x86_64.tar.gz"
      sha256 "05fbf28e332f766accbf1e851bd819fcbe6236de7b83d208517d693355e3c429"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-aarch64.tar.gz"
      sha256 "3bd72882444999652bd73913a44bfffc0bbb240cac0f16460d16efaa3728ade5"
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
