class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.103"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.103/wcr-macos-x86_64.tar.gz"
      sha256 "a96cf6ba6f047c2537a0bc8a5e0224d06450bb47e526a7d77726876d23dd2d67"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.103/wcr-macos-aarch64.tar.gz"
      sha256 "a1c6952a4bb6954dbb5eee24facb0a70b7c6c27b032c82f18b84d7ff29a22a55"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.103/wcr-linux-x86_64.tar.gz"
      sha256 "8427cf15bc4c362ba730444f47a7160a71dde126a2c81ab9b1e0e9278cbc5d21"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.103/wcr-linux-aarch64.tar.gz"
      sha256 "31db340de62d2978be8842e8b9194d07fe8c1679210351d70b4186de10216373"
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
