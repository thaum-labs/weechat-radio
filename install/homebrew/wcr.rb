class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.2"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.2/wcr-macos-x86_64.tar.gz"
      sha256 "d50f78e2cc076b78f702335910e924afe00912d7ed2ad4c9812c1596b305f8a8"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.2/wcr-macos-aarch64.tar.gz"
      sha256 "00e29c97cceaa85c4c4554ab93b04ab451e3a9a1f3262f0f937f46e1b5424307"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.2/wcr-linux-x86_64.tar.gz"
      sha256 "4ffbda6ef55b4c51dd370b8e9b1d334380978ddac75df4e5da2c7cba4e083c79"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.2/wcr-linux-aarch64.tar.gz"
      sha256 "8fba2b38d9484d54f35329cd206ab756afecf946b40232151a8d945ff0789d5d"
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
