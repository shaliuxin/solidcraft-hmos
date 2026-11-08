import native from 'libsolidcraft.so';
import { AbilityConstant, Configuration, UIAbility, Want } from '@kit.AbilityKit';
import { window } from '@kit.ArkUI';
import { BusinessError } from '@kit.BasicServicesKit';
import { publishSystemTheme, systemThemeFromColorMode } from '../SystemTheme';

class SystemThemeInput {
  kind: string = 'system_theme';
  text: string = '';
  constructor(text: string) { this.text = text; }
}

export default class EntryAbility extends UIAbility {
  private syncSystemTheme(colorMode: number | undefined): void {
    const theme = systemThemeFromColorMode(colorMode);
    publishSystemTheme(theme);
    // Push live changes into the native worker; init also carries the first value.
    try {
      native.input(JSON.stringify(new SystemThemeInput(theme)));
    } catch (error) {
      console.info('System theme input unavailable before native init');
    }
  }

  onCreate(want: Want, _launchParam: AbilityConstant.LaunchParam): void {
    // 冷启动（系统"用其它应用打开"）时 want 携带待打开文件的 URI，先存起来，
    // 等页面初始化后再消费；热启动的同一逻辑在 onNewWant 里。
    if (want?.uri) {
      AppStorage.setOrCreate('solidcraftOpenWant', want.uri);
    }
    this.syncSystemTheme(this.context.config?.colorMode);
    console.info('SolidCraft native ability created');
  }

  onNewWant(want: Want): void {
    // A file opened from the system shares its URI through startAbility; the page drains it on load.
    if (want?.uri) {
      AppStorage.setOrCreate('solidcraftOpenWant', want.uri);
    }
  }

  onConfigurationUpdate(newConfig: Configuration): void {
    this.syncSystemTheme(newConfig.colorMode);
  }

  onForeground(): void {
    native.active(true);
  }

  onBackground(): void {
    native.active(false);
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    if (AppStorage.get<string>('solidcraftSystemTheme') === undefined) {
      this.syncSystemTheme(this.context.config?.colorMode);
    }
    windowStage.loadContent('pages/Index', (error: BusinessError) => {
      if (error.code) {
        console.error(`SolidCraft page failed: ${error.code} ${error.message}`);
      }
    });
  }
}
