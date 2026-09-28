class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.79"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.79/wcr-macos-x86_64.tar.gz"
      sha256 "b969969694aefebd010c315ca7eff1dba4e6a46695e1b88c2ad8fece8eaad054"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.79/wcr-macos-aarch64.tar.gz"
      sha256 "21ed96ab7ef682a23734d4e37ffbf9b893f60ffd917f88a4ee960d12259dd384"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.79/wcr-linux-x86_64.tar.gz"
      sha256 "61e76e583641e473c7303fa0fd207f6ff42bd7ed8c4bb621917152a90c3b8946"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.79/wcr-linux-aarch64.tar.gz"
      sha256 "97e63aa251e49e701110f8622be5aef9c611d67627d9be98a1ccc84106892e79"
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
