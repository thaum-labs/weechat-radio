class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.95"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.95/wcr-macos-x86_64.tar.gz"
      sha256 "e90272043d2b82ef3f035d9ec7fe3160d66ce789ac7275b4be3d25de6e4234c9"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.95/wcr-macos-aarch64.tar.gz"
      sha256 "1065ed60dbe69100a291fdf86959df13113c3298c6f6593984e3b4eb898aab78"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.95/wcr-linux-x86_64.tar.gz"
      sha256 "fa2267b9723887d7052280baa3d2ea7faee8e10db08f4c90eb4814ddf883566e"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.95/wcr-linux-aarch64.tar.gz"
      sha256 "1a827d7cfd3d288055378a3f8c307d26b88f480e30835ba242a8327b88566bc3"
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
