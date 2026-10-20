class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.101"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.101/wcr-macos-x86_64.tar.gz"
      sha256 "d8e3bb7bdd9291661775bbca285fe18e598ca2995737559a7fdb60e305de2216"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.101/wcr-macos-aarch64.tar.gz"
      sha256 "614a16a971b5fbbccb48af9971b0f23ffd873482e5355d95a8db518fecedae12"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.101/wcr-linux-x86_64.tar.gz"
      sha256 "90a09df4fd018a31409f724a18e81a35745560c5c4eb48023acb5bb9f0677a1e"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.101/wcr-linux-aarch64.tar.gz"
      sha256 "ccdfcb87498c8dbea1d3c26ac6e8a7c6e093f890bc7a17beafc2af4d826a7482"
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
