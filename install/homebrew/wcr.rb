class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.10"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.10/wcr-macos-x86_64.tar.gz"
      sha256 "157a0f77623d57a9a64a18eb841d95ddfa4539973c530a2193698da1b10dbd93"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.10/wcr-macos-aarch64.tar.gz"
      sha256 "560b11aaae1093068b87f8f0a3dbc8192e6fc91ca7443e6029352d5b9f8bd2ed"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.10/wcr-linux-x86_64.tar.gz"
      sha256 "ea8d8d435549c8c4dc61484d6ed78b4fc9a14e88a0070f628fa7b34d7ccd4c96"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.10/wcr-linux-aarch64.tar.gz"
      sha256 "a96f138ba2b13f67c51bb1f579f38c1c6fe3d52858558c530a12c5a2439f933c"
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
