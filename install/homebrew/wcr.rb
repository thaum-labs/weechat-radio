class Wcr < Formula
  desc "WeeChat Radio — chat over internet and HF/VHF"
  homepage "https://weechatradio.com"
  version "1.0.6"
  license "Apache-2.0"

  on_macos do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.6/wcr-macos-x86_64.tar.gz"
      sha256 "2fdfed8edde1379cd5544bc8733ada6f9d65eada64bfb39ae8b672d5dd376e7a"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.6/wcr-macos-aarch64.tar.gz"
      sha256 "015173549920a24e301443c71c9d34a23e24587244fd42319e4f27c0b8d12a6c"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.6/wcr-linux-x86_64.tar.gz"
      sha256 "1c49b6237de4196065269d5f443b617c859bd7244c6bfb85099178d77fda1797"
    end
    on_arm do
      url "https://github.com/thaum-labs/weechat-radio/releases/download/v1.0.6/wcr-linux-aarch64.tar.gz"
      sha256 "081e56bbf802e8816312eb6b3c796f613bb76059abf4930b711facff187e4d66"
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
