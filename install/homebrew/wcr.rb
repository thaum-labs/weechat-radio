class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.81"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.81/wcr-macos-x86_64.tar.gz"
      sha256 "bf0498936cd952eb2f828f1d4a488afc35037511484d8689f6259d4beb56861a"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.81/wcr-macos-aarch64.tar.gz"
      sha256 "221dff9cdd8bfb470dcecdf17f2c9861713b25ec60752f2b7466fee539463ebf"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.81/wcr-linux-x86_64.tar.gz"
      sha256 "c72b011fad8d13853115929503f4d102aaa363b6dfddf21d0818c0fa81e90dd9"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.81/wcr-linux-aarch64.tar.gz"
      sha256 "c71b91e4c1e60da3fe83a70ee1573310a81d24aadad3e368c7d3be70a4c3c61e"
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
