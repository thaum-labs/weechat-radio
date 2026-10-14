class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.84"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.84/wcr-macos-x86_64.tar.gz"
      sha256 "8759c2d9bdf3efe69bd129f917101e37c819a1996c1498916139c8dd8a88e28d"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.84/wcr-macos-aarch64.tar.gz"
      sha256 "a46ca260f8d837b567fdf43bb0d7db944813052b635e726d0e840e3075bfa2df"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.84/wcr-linux-x86_64.tar.gz"
      sha256 "34cefbec8859b1c3530cc2a6625cd29e5fe20d829cd2b78a58248b28469ea0d6"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.84/wcr-linux-aarch64.tar.gz"
      sha256 "4f7263973ddb0481e1eb96f1580059ad0e0c27b058b84eafc79a4cf791c2516b"
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
