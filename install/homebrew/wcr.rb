class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.14"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-x86_64.tar.gz"
      sha256 "03260505f26f2bd0abd6f2f6fb2af68e3b3b6fe53008fa3f3cc75045cb854634"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-macos-aarch64.tar.gz"
      sha256 "5986d7681e3b539b2cddf2334197df3fa529933a4361650df85c5de416a3aa0f"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-x86_64.tar.gz"
      sha256 "62309453ec1f01f00d5a3b651dcaeb4ec222bd2e6d5fae93be6ea5bc2bb5fa94"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.14/wcr-linux-aarch64.tar.gz"
      sha256 "67dcea2b11c2c23ad61e8798f6701977e2e80721660c6298eff6a1413ae797e0"
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
