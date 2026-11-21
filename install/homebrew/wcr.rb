class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.21"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.21/wcr-macos-x86_64.tar.gz"
      sha256 "f7cb750f1e962f91565244746bc3dd6fe37a58f694425cb8a25ed1b2c155950f"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.21/wcr-macos-aarch64.tar.gz"
      sha256 "60a803b8266f9fc825d7c5e5ff17b471d78e575f0f91fb0773073c229c3738ad"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.21/wcr-linux-x86_64.tar.gz"
      sha256 "df7af145850327f184f87ea8ff9b25d84900e2afd0b49eda0da14d34d24b7fe3"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.21/wcr-linux-aarch64.tar.gz"
      sha256 "a72c20bc4b654b827a2c433be9eb55509ade3a1abc90f64c70c8c6de5b6036c3"
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
