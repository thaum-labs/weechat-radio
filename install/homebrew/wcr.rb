class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.17"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.17/wcr-macos-x86_64.tar.gz"
      sha256 "8b24beff212532cac9a653ef31cb491b90b29b744f17731fb89088b87869c641"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.17/wcr-macos-aarch64.tar.gz"
      sha256 "41dc594b57a0c63a4af202fa5580aca57de7654cbfacb172447b460b4383a5b5"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.17/wcr-linux-x86_64.tar.gz"
      sha256 "4ab933b30de6c15cc01c09d9965d51738b6b941f2e5c7ac660732946f715b86d"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.17/wcr-linux-aarch64.tar.gz"
      sha256 "53f2b634defa0c0c101813d6fc120b1ca821aacf14d7951797aecdcd1233f6fa"
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
