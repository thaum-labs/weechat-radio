class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.80"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.80/wcr-macos-x86_64.tar.gz"
      sha256 "361a5e5fd27aa793e0cb36a50e2c7cff669f2e0a37db90c18929214f7037fe81"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.80/wcr-macos-aarch64.tar.gz"
      sha256 "d17564b685c3ce58f4126efee1e22204e51f61394c688979a3446327256c0e7a"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.80/wcr-linux-x86_64.tar.gz"
      sha256 "5e027cbf4cbd90c65bacc0c9e2b1acc6e200b1bad53b1406190f134e9b455f49"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.80/wcr-linux-aarch64.tar.gz"
      sha256 "a7e8c1faa6340c4c1f5cff8518f024697caa0ae7396854adacbc98f468518372"
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
