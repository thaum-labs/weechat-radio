class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.4"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.4/wcr-macos-x86_64.tar.gz"
      sha256 "551cc63047d5df667568db7ee9eb92460638db5348a1b445f037cd1e49e6ec3f"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.4/wcr-macos-aarch64.tar.gz"
      sha256 "eebda72f58adc6ad8440c7a7facf92c510785c23544e251d32169064f822d304"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.4/wcr-linux-x86_64.tar.gz"
      sha256 "1b3195b5f961ea3ba15f4ccb813bfdf04d2d228f703a26237e579e1ecab7620a"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.4/wcr-linux-aarch64.tar.gz"
      sha256 "b1feb90cbe61bd63f0e69b721001e52ffe41dd18a06454b0549f8f9187e8d70b"
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
