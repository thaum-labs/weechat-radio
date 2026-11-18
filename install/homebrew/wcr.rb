class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.19"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.19/wcr-macos-x86_64.tar.gz"
      sha256 "a2f54def2c18f32c3ecb2b311e34f996034b2f123982c0bcf4f0e8841658ace7"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.19/wcr-macos-aarch64.tar.gz"
      sha256 "9be5bf6608ce5e664eebd1fb4c9c38c76d485e60b9bf70e2018a98b0ff484d62"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.19/wcr-linux-x86_64.tar.gz"
      sha256 "935f5aea1e73b3d29fdc20aad43148caac55b4152cc4fbe65635d5f70bb71a93"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.19/wcr-linux-aarch64.tar.gz"
      sha256 "98143ecd2e1cb4306cfe846cb497a18da4cc58861c23b307365273a83b766b1e"
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
