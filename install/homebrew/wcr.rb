class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.15"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.15/wcr-macos-x86_64.tar.gz"
      sha256 "fa50a04159ef3bb4040cce6c4f3c420c9076cd957e0c86811aa3ba9201ce5366"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.15/wcr-macos-aarch64.tar.gz"
      sha256 "8943f1e9748ac451c41dbd2e3a7534401c72b88437744caa6c4a24e2659c2b19"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.15/wcr-linux-x86_64.tar.gz"
      sha256 "1980a62d000274bc05daaa2639d4e6714836181ae3866d81177b85f7550e068a"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.15/wcr-linux-aarch64.tar.gz"
      sha256 "92716f1c8a849b1f1265a88f99352d9016598f7f325cb179c72740a3498ab662"
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
