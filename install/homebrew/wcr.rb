class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.96"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.96/wcr-macos-x86_64.tar.gz"
      sha256 "7238ddc1e601ee1cad893fc87ad01f7b706c81adf83a7aefb51334603ddbcd6e"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.96/wcr-macos-aarch64.tar.gz"
      sha256 "9c191215a154ae8a2278baa8a5fde59e2132adccd761dbf7c6b55fa085e51bf5"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.96/wcr-linux-x86_64.tar.gz"
      sha256 "e3c038cc85d469641753a76886c8526e77f5f7dbf7f46b4925bead57e6ec7850"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.96/wcr-linux-aarch64.tar.gz"
      sha256 "8fa767f7ed088007e4e3f839d4a244ce4013c3cdef53ce628a8a27b2b9df5c3c"
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
