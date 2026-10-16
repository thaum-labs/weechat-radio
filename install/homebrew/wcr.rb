class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "0.1.92"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.92/wcr-macos-x86_64.tar.gz"
      sha256 "c63ba5bdf45987d41d12a4966eec6332274a5180d663e26ad22c996d2139e7c7"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.92/wcr-macos-aarch64.tar.gz"
      sha256 "197a7476cbf0c32121d266cf371b01a0d715995421051d0b60bbe3092a5c797a"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.92/wcr-linux-x86_64.tar.gz"
      sha256 "27e5003a9c72ed789e178abbe47cf1497dc928f7e4a31714655f8dfff25c9da0"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v0.1.92/wcr-linux-aarch64.tar.gz"
      sha256 "1ca3094b37666bece7b83b71f8b863c76cb2864e45558241fdcca3c0c90a0f0c"
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
