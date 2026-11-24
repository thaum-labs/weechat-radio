class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.23"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.23/wcr-macos-x86_64.tar.gz"
      sha256 "8df62574a419b54a5c0675541bee97bd9b4ee078acefeaf6ae194fdbe21d4acb"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.23/wcr-macos-aarch64.tar.gz"
      sha256 "1d5ac65c51813a488aed01e0647d41b8f18f74718e216c426d21883bcf6c5698"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.23/wcr-linux-x86_64.tar.gz"
      sha256 "161e797a00efb2558057ff86e7f4ccea4b1f5dd53c947efb83e510ba33d8dcb5"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.23/wcr-linux-aarch64.tar.gz"
      sha256 "d2461220da60f2418bc4b34dc73a7cf2350d706bbb63e92d4730f36e13b75ed0"
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
